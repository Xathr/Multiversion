//! Packet framing: length prefix, optional zlib compression, optional AES/CFB8 encryption.
//! Works on byte buffers fed by the reactor; no blocking I/O here.

use super::buf::{varint_len, Reader};
#[allow(unused_imports)]
use std::convert::TryInto;
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};


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
    /// zlib state, about 40 KiB. Made when the first compressed packet arrives: most
    /// connections (server-list pings, logins that fail, an uncompressed link to Pumpkin) never
    /// need it.
    inflater: Option<Box<Decompress>>,
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
        Self { buf: Vec::new(), read: 0, cipher: None, compression: -1, inflater: None, max_frame: MAX_FRAME, keep_compressed: false }
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
            let marker = r.position();
            let mut frame = frame;
            frame.drain(..marker);
            return Ok(frame);
        }
        if data_len < 0 || data_len as usize > self.max_frame {
            return Err(format!("invalid uncompressed length {data_len}"));
        }
        let mut out = Vec::with_capacity(data_len as usize + 16);
        let inflater = self.inflater.get_or_insert_with(|| Box::new(Decompress::new(true)));
        inflater.reset(true);
        let input = r.remaining();
        loop {
            let consumed = inflater.total_in() as usize;
            let before_out = inflater.total_out();
            let status = inflater
                .decompress_vec(&input[consumed..], &mut out, FlushDecompress::Finish)
                .map_err(|e| format!("zlib: {e}"))?;
            match status {
                Status::StreamEnd => break,
                Status::Ok | Status::BufError => {
                    if out.len() > data_len as usize {
                        return Err(format!("decompressed more than the declared {data_len} bytes"));
                    }
                    if inflater.total_out() == before_out && inflater.total_in() as usize == consumed {
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

/// One direction of a connection's encryption.
pub use super::aes::Cfb8 as Cfb8Stream;

/// Builds outbound frames (compress → length prefix → encrypt).
pub struct Outbound {
    pub compression: i32,
    /// zlib state, about 300 KiB. Made when the first packet is compressed, for the same reason
    /// as `Inbound::inflater`.
    deflater: Option<Box<Compress>>,
    cipher: Option<Cfb8Stream>,
    /// Where a packet is compressed before its length is known; reused from packet to packet.
    scratch: Vec<u8>,
}

impl Default for Outbound {
    fn default() -> Self {
        Self { compression: -1, deflater: None, cipher: None, scratch: Vec::new() }
    }
}

impl Outbound {
    /// zlib-compresses `parts`, one after another as one stream, onto the end of `out`.
    /// `compress_vec` only fills the spare capacity of the output vector, so keep going
    /// (growing it) until the stream is finished.
    fn deflate_into(&mut self, parts: [&[u8]; 2], out: &mut Vec<u8>) {
        let deflater = self.deflater.get_or_insert_with(|| Box::new(Compress::new(Compression::new(1), true)));
        deflater.reset();
        let total: usize = parts.iter().map(|p| p.len()).sum();
        out.reserve(total / 2 + 64);
        let last = parts.len() - 1;
        for (i, part) in parts.iter().enumerate() {
            let flush = if i == last { FlushCompress::Finish } else { FlushCompress::None };
            let start_in = deflater.total_in();
            loop {
                let before_in = deflater.total_in();
                let before_out = deflater.total_out();
                let taken = (before_in - start_in) as usize;
                let status = deflater.compress_vec(&part[taken..], out, flush).expect("zlib compression");
                let all_in = (deflater.total_in() - start_in) as usize == part.len();
                match status {
                    Status::StreamEnd => break,
                    // a part that is not the last is done once it has all been taken in
                    Status::Ok | Status::BufError if i != last && all_in && out.len() < out.capacity() => break,
                    Status::Ok | Status::BufError => {
                        if deflater.total_out() == before_out && deflater.total_in() == before_in {
                            out.reserve(total + 64);
                        } else if out.len() == out.capacity() {
                            out.reserve(out.capacity().max(1024));
                        }
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
        let mut head = [0u8; 5];
        let n = put_varint(&mut head, body.len() as i32);
        out.reserve(n + body.len());
        out.extend_from_slice(&head[..n]);
        out.extend_from_slice(body);
    }
    /// Frames one packet (id+payload) into `out`.
    pub fn frame(&mut self, packet: &[u8], out: &mut Vec<u8>) {
        self.frame_parts(packet, &[], out);
    }

    /// Frames a packet given as its id and its payload, without first joining the two.
    pub fn frame_packet(&mut self, id: i32, payload: &[u8], out: &mut Vec<u8>) {
        let mut head = [0u8; 5];
        let n = put_varint(&mut head, id);
        self.frame_parts(&head[..n], payload, out);
    }

    /// Frames `first` followed by `rest` as one packet, written straight onto the end of `out`.
    fn frame_parts(&mut self, first: &[u8], rest: &[u8], out: &mut Vec<u8>) {
        let len = first.len() + rest.len();
        let mut head = [0u8; 5];
        if self.compression < 0 {
            let n = put_varint(&mut head, len as i32);
            out.reserve(n + len);
            out.extend_from_slice(&head[..n]);
            out.extend_from_slice(first);
            out.extend_from_slice(rest);
            return;
        }
        if len < self.compression as usize {
            let n = put_varint(&mut head, len as i32 + 1);
            out.reserve(n + 1 + len);
            out.extend_from_slice(&head[..n]);
            out.push(0);
            out.extend_from_slice(first);
            out.extend_from_slice(rest);
            return;
        }
        // The frame's length comes before the compressed data but is only known after it, so
        // the data is compressed into a buffer kept for the purpose and then added.
        let mut compressed = std::mem::take(&mut self.scratch);
        compressed.clear();
        self.deflate_into([first, rest], &mut compressed);
        let inner_len = varint_len(len as i32) + compressed.len();
        let n = put_varint(&mut head, inner_len as i32);
        out.reserve(n + inner_len);
        out.extend_from_slice(&head[..n]);
        let n = put_varint(&mut head, len as i32);
        out.extend_from_slice(&head[..n]);
        out.extend_from_slice(&compressed);
        // a chunk's worth is kept for the next packet; a rare huge one is not held on to
        if compressed.capacity() <= 1 << 20 {
            self.scratch = compressed;
        }
    }
}

/// Writes `value` as a varint at the start of `buf` and returns how many bytes it took.
fn put_varint(buf: &mut [u8; 5], value: i32) -> usize {
    let mut v = value as u32;
    let mut n = 0;
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            buf[n] = byte;
            return n + 1;
        }
        buf[n] = byte | 0x80;
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::buf::Writer;
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
    #[test]
    fn a_packet_framed_in_two_parts_is_the_packet_framed_whole() {
        // sizes around the compression threshold, an empty payload, and a chunk's worth
        for threshold in [-1, 0, 64, 256] {
            for size in [0usize, 1, 62, 63, 64, 65, 255, 256, 257, 5000, 300_000] {
                let payload: Vec<u8> = (0..size).map(|i| ((i * 7) ^ (i >> 5)) as u8).collect();
                for id in [0i32, 0x26, 0x7f, 0x80, 300] {
                    let mut whole = Writer::new();
                    whole.varint(id).raw(&payload);
                    let mut a = Outbound::default();
                    a.compression = threshold;
                    let mut b = Outbound::default();
                    b.compression = threshold;
                    let (mut wire_a, mut wire_b) = (vec![9u8], vec![9u8]);
                    a.frame(&whole.buf, &mut wire_a);
                    b.frame_packet(id, &payload, &mut wire_b);
                    // what was already in the output stays, and both ways give a frame that
                    // reads back as the packet
                    assert_eq!(wire_a[0], 9);
                    for wire in [&wire_a, &wire_b] {
                        let mut ib = Inbound { compression: threshold, ..Default::default() };
                        ib.feed(&wire[1..]);
                        assert_eq!(ib.next_packet().unwrap().as_deref(), Some(&whole.buf[..]), "threshold {threshold}, size {size}, id {id}");
                        assert_eq!(ib.next_packet().unwrap(), None);
                    }
                    if threshold < 0 || whole.buf.len() < threshold as usize {
                        assert_eq!(wire_a, wire_b);
                    }
                }
            }
        }
    }

    #[test]
    fn several_packets_share_one_compressor_and_buffer() {
        let mut out = Outbound::default();
        out.compression = 32;
        let mut wire = Vec::new();
        let packets: Vec<Vec<u8>> = (0..40usize).map(|n| (0..n * 97).map(|i| (i % 251) as u8).collect()).collect();
        for (i, p) in packets.iter().enumerate() {
            out.frame_packet(i as i32, p, &mut wire);
        }
        let mut ib = Inbound { compression: 32, ..Default::default() };
        // fed a few bytes at a time, as a slow connection delivers them
        for piece in wire.chunks(13) {
            ib.feed(piece);
        }
        for (i, p) in packets.iter().enumerate() {
            let got = ib.next_packet().unwrap().unwrap();
            let mut want = Writer::new();
            want.varint(i as i32).raw(p);
            assert_eq!(got, want.buf, "packet {i}");
        }
        assert_eq!(ib.next_packet().unwrap(), None);
    }

}
