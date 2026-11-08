//! Packet framing: length prefix, optional zlib compression, optional AES/CFB8 encryption.
//! Works on byte buffers fed by the reactor; no blocking I/O here.

use super::buf::{varint_len, Reader, Writer};
use aes::cipher::{AsyncStreamCipher, KeyIvInit};
#[allow(unused_imports)]
use std::convert::TryInto;
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

type Aes128Cfb8Enc = cfb8::Encryptor<aes::Aes128>;
type Aes128Cfb8Dec = cfb8::Decryptor<aes::Aes128>;

pub const MAX_PACKET: usize = 1 << 21; // vanilla: 2 MiB (uncompressed) / 8 MiB compressed cap
/// The largest frame accepted once a player is logged in (and from Pumpkin).
pub const MAX_FRAME: usize = MAX_PACKET * 4;
/// The largest frame a client may send before it has logged in. Handshake, status and login
/// packets are a few hundred bytes; without this, anyone could make the plugin hold 8 MiB per
/// connection before proving anything.
pub const MAX_FRAME_BEFORE_LOGIN: usize = 1 << 16;

/// One direction of a connection's byte pipeline (decrypt → split frames → decompress).
pub struct Inbound {
    buf: Vec<u8>,
    /// Bytes of `buf` already handed out as packets. They are dropped in one go now and then
    /// rather than after every packet, which would shift the whole buffer each time.
    read: usize,
    cipher: Option<Cfb8Stream>,
    pub compression: i32, // -1 = off
    inflater: Decompress,
    /// Frames (and declared decompressed sizes) above this are refused.
    pub max_frame: usize,
    /// `next_frame` hands compressed frames back as they are instead of decompressing them, for
    /// relaying to a client that uses the same compression threshold.
    pub keep_compressed: bool,
}

/// One frame from `Inbound::next_frame`.
pub enum Frame {
    /// A decoded packet (id + payload).
    Packet(Vec<u8>),
    /// A compressed frame's body (uncompressed length + zlib data), left as it arrived.
    Compressed(Vec<u8>),
}

impl Default for Inbound {
    fn default() -> Self {
        Self { buf: Vec::new(), read: 0, cipher: None, compression: -1, inflater: Decompress::new(true), max_frame: MAX_FRAME, keep_compressed: false }
    }
}

impl Inbound {
    /// An inbound pipeline that refuses frames above `max_frame` bytes.
    pub fn with_max_frame(max_frame: usize) -> Self {
        Self { max_frame, ..Self::default() }
    }
    pub fn enable_encryption(&mut self, key: [u8; 16]) {
        self.cipher = Some(Cfb8Stream::new(key));
    }
    /// Feeds raw bytes from the socket (decrypting as they arrive).
    pub fn feed(&mut self, data: &[u8]) {
        if self.read > 0 && (self.read == self.buf.len() || self.read >= 1 << 16) {
            self.buf.drain(..self.read);
            self.read = 0;
        }
        let start = self.buf.len();
        self.buf.extend_from_slice(data);
        if let Some(c) = self.cipher.as_mut() {
            c.decrypt(&mut self.buf[start..]);
        }
    }
    pub fn buffered(&self) -> usize {
        self.buf.len() - self.read
    }
    /// Pops the next frame: a decoded packet, or with `keep_compressed`, a compressed frame as is.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, String> {
        let Some(frame) = self.take_frame()? else { return Ok(None) };
        if self.keep_compressed && self.compression >= 0 {
            let mut r = Reader::new(&frame);
            let data_len = r.varint().map_err(|e| e.to_string())?;
            if data_len != 0 {
                if data_len < 0 || data_len as usize > self.max_frame {
                    return Err(format!("invalid uncompressed length {data_len}"));
                }
                return Ok(Some(Frame::Compressed(frame)));
            }
        }
        self.decode(frame).map(|p| Some(Frame::Packet(p)))
    }

    /// Pops the next complete, decompressed packet (id + payload) if available.
    pub fn next_packet(&mut self) -> Result<Option<Vec<u8>>, String> {
        let Some(frame) = self.take_frame()? else { return Ok(None) };
        self.decode(frame).map(Some)
    }

    /// The next complete frame's bytes (after the length prefix), if it has fully arrived.
    fn take_frame(&mut self) -> Result<Option<Vec<u8>>, String> {
        let (len, hdr) = {
            let pending = &self.buf[self.read..];
            let mut r = Reader::new(pending);
            match r.varint() {
                Ok(len) => (len, r.position()),
                Err(_) if pending.len() < 5 => return Ok(None),
                Err(e) => return Err(format!("bad frame length: {e}")),
            }
        };
        if len <= 0 || len as usize > self.max_frame {
            return Err(format!("invalid frame length {len}"));
        }
        let len = len as usize;
        if self.buf.len() - self.read < hdr + len {
            return Ok(None);
        }
        let frame: Vec<u8> = self.buf[self.read + hdr..self.read + hdr + len].to_vec();
        self.read += hdr + len;
        Ok(Some(frame))
    }

    /// Decompresses a frame (when compression is on) into id + payload.
    fn decode(&mut self, frame: Vec<u8>) -> Result<Vec<u8>, String> {
        if self.compression < 0 {
            return Ok(frame);
        }
        let mut r = Reader::new(&frame);
        let data_len = r.varint().map_err(|e| e.to_string())?;
        if data_len == 0 {
            return Ok(r.remaining().to_vec());
        }
        if data_len < 0 || data_len as usize > self.max_frame {
            return Err(format!("invalid uncompressed length {data_len}"));
        }
        let mut out = Vec::with_capacity(data_len as usize + 16);
        self.inflater.reset(true);
        let input = r.remaining();
        loop {
            let consumed = self.inflater.total_in() as usize;
            let before_out = self.inflater.total_out();
            let status = self.inflater
                .decompress_vec(&input[consumed..], &mut out, FlushDecompress::Finish)
                .map_err(|e| format!("zlib: {e}"))?;
            match status {
                Status::StreamEnd => break,
                Status::Ok | Status::BufError => {
                    if out.len() > data_len as usize {
                        return Err(format!("decompressed more than the declared {data_len} bytes"));
                    }
                    if self.inflater.total_out() == before_out && self.inflater.total_in() as usize == consumed {
                        return Err("truncated compressed packet".into());
                    }
                    if out.len() == out.capacity() {
                        out.reserve(1024);
                    }
                }
            }
        }
        if out.len() != data_len as usize {
            return Err(format!("decompressed {} bytes, expected {data_len}", out.len()));
        }
        Ok(out)
    }
}

/// Encryption-aware inbound: keeps ciphertext state correctly. Replaces `Inbound::feed` when
/// encryption is on. (Kept separate so the hot path without encryption stays simple.)
pub struct Cfb8Stream {
    key: [u8; 16],
    iv: [u8; 16],
}

impl Cfb8Stream {
    pub fn new(key: [u8; 16]) -> Self {
        Self { key, iv: key }
    }
    /// Decrypts `data` in place.
    pub fn decrypt(&mut self, data: &mut [u8]) {
        if data.is_empty() {
            return;
        }
        let mut last_iv = self.iv;
        // Next IV = last 16 ciphertext bytes (with previous IV as prefix if fewer).
        let mut tail = [0u8; 16];
        let combined: Vec<u8> = last_iv.iter().copied().chain(data.iter().copied()).collect();
        tail.copy_from_slice(&combined[combined.len() - 16..]);
        Aes128Cfb8Dec::new((&self.key).into(), (&last_iv).into()).decrypt(data);
        last_iv = tail;
        self.iv = last_iv;
    }
    /// Encrypts `data` in place.
    pub fn encrypt(&mut self, data: &mut [u8]) {
        if data.is_empty() {
            return;
        }
        Aes128Cfb8Enc::new((&self.key).into(), (&self.iv).into()).encrypt(data);
        let combined: Vec<u8> = self.iv.iter().copied().chain(data.iter().copied()).collect();
        self.iv.copy_from_slice(&combined[combined.len() - 16..]);
    }
}

/// Builds outbound frames (compress → length prefix → encrypt).
pub struct Outbound {
    pub compression: i32,
    deflater: Compress,
    cipher: Option<Cfb8Stream>,
}

impl Default for Outbound {
    fn default() -> Self {
        Self { compression: -1, deflater: Compress::new(Compression::new(1), true), cipher: None }
    }
}

impl Outbound {
    /// zlib-compresses `data` completely. `compress_vec` only fills the spare capacity of the
    /// output vector, so keep going (growing it) until the stream is finished.
    fn deflate(&mut self, data: &[u8]) -> Vec<u8> {
        self.deflater.reset();
        let mut out = Vec::with_capacity(data.len() / 2 + 64);
        loop {
            let before_in = self.deflater.total_in();
            let before_out = self.deflater.total_out();
            let status = self.deflater.compress_vec(&data[before_in as usize..], &mut out, FlushCompress::Finish).expect("zlib compression");
            match status {
                Status::StreamEnd => return out,
                Status::Ok | Status::BufError => {
                    if self.deflater.total_out() == before_out && self.deflater.total_in() == before_in {
                        out.reserve(data.len() + 64);
                    } else if out.len() == out.capacity() {
                        out.reserve(out.capacity().max(1024));
                    }
                }
            }
        }
    }

    pub fn enable_encryption(&mut self, key: [u8; 16]) {
        self.cipher = Some(Cfb8Stream::new(key));
    }
    /// Encrypts bytes that `frame` appended, right before they go to the socket.
    pub fn seal(&mut self, out: &mut [u8]) {
        if let Some(c) = self.cipher.as_mut() {
            c.encrypt(out);
        }
    }
    /// Frames an already-compressed frame body (from `Frame::Compressed`) into `out`.
    pub fn frame_compressed(&mut self, body: &[u8], out: &mut Vec<u8>) {
        let mut w = Writer::with_capacity(body.len() + 5);
        w.varint(body.len() as i32);
        w.raw(body);
        out.extend_from_slice(&w.buf);
    }
    /// Frames one packet (id+payload) into `out`.
    pub fn frame(&mut self, packet: &[u8], out: &mut Vec<u8>) {
        if self.compression < 0 {
            let mut w = Writer::with_capacity(packet.len() + 5);
            w.varint(packet.len() as i32);
            w.raw(packet);
            out.extend_from_slice(&w.buf);
            return;
        }
        if packet.len() < self.compression as usize {
            let mut w = Writer::with_capacity(packet.len() + 6);
            w.varint(packet.len() as i32 + 1);
            w.varint(0);
            w.raw(packet);
            out.extend_from_slice(&w.buf);
            return;
        }
        let compressed = self.deflate(packet);
        let inner_len = varint_len(packet.len() as i32) + compressed.len();
        let mut w = Writer::with_capacity(inner_len + 5);
        w.varint(inner_len as i32);
        w.varint(packet.len() as i32);
        w.raw(&compressed);
        out.extend_from_slice(&w.buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressed_frames_pass_through_unchanged() {
        let mut from_pumpkin = Outbound { compression: 64, ..Default::default() };
        let big: Vec<u8> = (0..5000u32).map(|i| (i % 7) as u8).collect();
        let small = vec![3u8, 1, 2];
        let mut wire = Vec::new();
        from_pumpkin.frame(&big, &mut wire);
        from_pumpkin.frame(&small, &mut wire);

        let mut ib = Inbound { compression: 64, keep_compressed: true, ..Default::default() };
        ib.feed(&wire);
        let mut to_client = Outbound { compression: 64, ..Default::default() };
        let mut relayed = Vec::new();
        match ib.next_frame().unwrap() {
            Some(Frame::Compressed(body)) => to_client.frame_compressed(&body, &mut relayed),
            _ => panic!("the large packet should stay compressed"),
        }
        match ib.next_frame().unwrap() {
            Some(Frame::Packet(p)) => {
                assert_eq!(p, small, "small packets are decoded so their ids can be read");
                to_client.frame(&p, &mut relayed);
            }
            _ => panic!("the small packet should be decoded"),
        }
        assert_eq!(relayed, wire, "the client gets exactly what Pumpkin sent");

        let mut client = Inbound { compression: 64, ..Default::default() };
        client.feed(&relayed);
        assert_eq!(client.next_packet().unwrap(), Some(big));
        assert_eq!(client.next_packet().unwrap(), Some(small));
    }
    #[test]
    fn frames_above_the_limit_are_refused() {
        let mut ib = Inbound { max_frame: MAX_FRAME_BEFORE_LOGIN, ..Default::default() };
        let mut w = Writer::new();
        w.varint(MAX_FRAME_BEFORE_LOGIN as i32 + 1);
        ib.feed(&w.into_inner());
        assert!(ib.next_packet().is_err(), "a frame over the pre-login limit is refused from its length alone");
        let mut ib = Inbound { max_frame: MAX_FRAME_BEFORE_LOGIN, ..Default::default() };
        let mut w = Writer::new();
        w.varint(3);
        w.raw(&[0, 1, 2]);
        ib.feed(&w.into_inner());
        assert_eq!(ib.next_packet().unwrap(), Some(vec![0, 1, 2]));
    }
    #[test]
    fn many_small_packets_in_one_burst() {
        let mut ob = Outbound::default();
        let mut bytes = Vec::new();
        for i in 0..20_000u32 {
            ob.frame(&i.to_be_bytes(), &mut bytes);
        }
        let mut ib = Inbound::default();
        // Arrives in uneven pieces, as from a socket.
        for chunk in bytes.chunks(7_777) {
            ib.feed(chunk);
        }
        for i in 0..20_000u32 {
            assert_eq!(ib.next_packet().unwrap(), Some(i.to_be_bytes().to_vec()));
        }
        assert_eq!(ib.next_packet().unwrap(), None);
        assert_eq!(ib.buffered(), 0);
        ib.feed(&[]);
        assert!(ib.buf.is_empty(), "consumed bytes are dropped on the next feed");
    }
    #[test]
    fn frame_roundtrip_plain_and_compressed() {
        for threshold in [-1, 0, 256] {
            let mut ob = Outbound { compression: threshold, ..Default::default() };
            let mut ib = Inbound { compression: threshold, ..Default::default() };
            let small = vec![7u8; 10];
            let big = vec![3u8; 5000];
            let mut wire = Vec::new();
            ob.frame(&small, &mut wire);
            ob.frame(&big, &mut wire);
            ib.feed(&wire[..3]);
            assert!(ib.next_packet().unwrap().is_none() || wire.len() < 4);
            ib.feed(&wire[3..]);
            assert_eq!(ib.next_packet().unwrap().unwrap(), small);
            assert_eq!(ib.next_packet().unwrap().unwrap(), big);
            assert!(ib.next_packet().unwrap().is_none());
        }
    }
    #[test]
    fn incompressible_packets_survive_compression() {
        // Pseudo-random bytes don't shrink under zlib; the frame must still carry all of them.
        let mut x: u32 = 0x1234_5678;
        let data: Vec<u8> = (0..1146)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect();
        let mut ob = Outbound { compression: 256, ..Default::default() };
        let mut ib = Inbound { compression: 256, ..Default::default() };
        let mut wire = Vec::new();
        ob.frame(&data, &mut wire);
        ib.feed(&wire);
        assert_eq!(ib.next_packet().unwrap().unwrap(), data);
        // and a large one that compresses well
        let big = vec![7u8; 200_000];
        let mut wire = Vec::new();
        ob.frame(&big, &mut wire);
        ib.feed(&wire);
        assert_eq!(ib.next_packet().unwrap().unwrap(), big);
    }

    #[test]
    fn cfb8_roundtrip() {
        let key = [9u8; 16];
        let mut enc = Cfb8Stream::new(key);
        let mut dec = Cfb8Stream::new(key);
        let msg = b"hello minecraft encryption stream".to_vec();
        let mut a = msg[..5].to_vec();
        let mut b = msg[5..].to_vec();
        enc.encrypt(&mut a);
        enc.encrypt(&mut b);
        let mut c = [a.clone(), b.clone()].concat();
        // decrypt in different chunking
        let (x, y) = c.split_at_mut(17);
        dec.decrypt(x);
        dec.decrypt(y);
        assert_eq!(c, msg);
    }
}
