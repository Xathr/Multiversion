//! 1.17.1 (protocol 756) ↔ 1.17 (protocol 755).
//!
//! The same game, with three packets laid out differently. What 1.17.1 changed:
//! - Several entities are removed with one packet; 1.17 takes one entity per packet.
//! - Inventories carry a "state" number that the client sends back with each click, and the
//!   whole-inventory packet also says what is on the cursor. 1.17 has neither: the number is
//!   kept here and added to the client's clicks, and the cursor is sent as its own packet.
//! - A book is edited by sending its pages; 1.17 sends the whole book item.

use super::{Packet, State, TResult, Translator};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};
use crate::proto::version::Version;
use std::collections::HashMap;

// Clientbound play packets (the same numbers in both versions).
const CONTAINER_CLOSE: i32 = 19;
const CONTAINER_SET_CONTENT: i32 = 20;
const CONTAINER_SET_SLOT: i32 = 22;
const REMOVE_ENTITIES: i32 = 58;

// Serverbound play packets.
const CONTAINER_CLICK: i32 = 8;
const CONTAINER_CLOSE_S: i32 = 9;
const EDIT_BOOK: i32 = 11;

// What the server accepts of a book.
const MAX_PAGES: usize = 100;
const MAX_PAGE_CHARS: usize = 8192;
const MAX_TITLE_CHARS: usize = 128;

fn err(msg: impl Into<String>) -> DecodeError {
    DecodeError::Other(msg.into())
}

/// Copies one item stack unchanged: present, id, count, data.
fn item_copy(r: &mut Reader, w: &mut Writer) -> Result<()> {
    r.copy_to(w, |r| {
        if r.bool()? {
            r.varint()?;
            r.i8()?;
            nbt::skip_named_optional(r)?;
        }
        Ok(())
    })
}

fn cut(text: &str, max: usize) -> &str {
    text.char_indices().nth(max).map_or(text, |(at, _)| &text[..at])
}

/// REMOVE_ENTITIES: the ids, each to go out in a packet of its own.
fn removed_entities(p: &[u8]) -> Result<Vec<i32>> {
    let mut r = Reader::new(p);
    let n = usize::try_from(r.varint()?).map_err(|_| err("negative count"))?;
    if n > r.len() {
        return Err(err("more entities than bytes"));
    }
    (0..n).map(|_| r.varint()).collect()
}

/// CONTAINER_SET_SLOT without its state number: (window, state, packet).
fn container_set_slot(p: &[u8]) -> Result<(i8, i32, Vec<u8>)> {
    let mut r = Reader::new(p);
    let window = r.i8()?;
    let state = r.varint()?;
    let mut w = Writer::with_capacity(p.len());
    w.i8(window).raw(r.remaining());
    Ok((window, state, w.into_inner()))
}

/// CONTAINER_SET_CONTENT as 1.17 has it, and the cursor's item as a packet of its own:
/// (window, state, packet, cursor packet).
fn container_set_content(p: &[u8]) -> Result<(i8, i32, Vec<u8>, Vec<u8>)> {
    let mut r = Reader::new(p);
    let window = r.u8()?;
    let state = r.varint()?;
    let n = r.varint()?;
    let count = i16::try_from(n).map_err(|_| err(format!("{n} slots")))?;
    let mut w = Writer::with_capacity(p.len());
    w.u8(window).i16(count);
    for _ in 0..count.max(0) {
        item_copy(&mut r, &mut w)?;
    }
    // window -1, slot -1 is the cursor
    let mut cursor = Writer::new();
    cursor.i8(-1).i16(-1);
    item_copy(&mut r, &mut cursor)?;
    Ok((window as i8, state, w.into_inner(), cursor.into_inner()))
}

/// CONTAINER_CLICK with the state number the server last gave for that window.
fn container_click(p: &[u8], state: i32) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len() + 5);
    w.u8(r.u8()?).varint(state).raw(r.remaining());
    Ok(w.into_inner())
}

/// EDIT_BOOK: 1.17 sends the book; the server wants the slot, the pages and, when the book is
/// being signed, its title.
fn edit_book(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let tag = if r.bool()? {
        r.varint()?;
        r.i8()?;
        nbt::read_named_optional(&mut r)?
    } else {
        None
    };
    let signing = r.bool()?;
    let slot = r.varint()?;
    let mut w = Writer::new();
    w.varint(slot);
    let pages: Vec<&str> = tag.as_ref().and_then(|t| t.get("pages")).and_then(Tag::as_list).map(|l| l.iter().filter_map(Tag::as_str).take(MAX_PAGES).collect()).unwrap_or_default();
    let title = tag.as_ref().and_then(|t| t.get("title")).and_then(Tag::as_str);
    if tag.is_none() || signing && title.is_none() {
        // nothing usable: an empty edit
        w.varint(0).bool(false);
        return Ok(w.into_inner());
    }
    w.varint(pages.len() as i32);
    for page in pages {
        w.string(cut(page, MAX_PAGE_CHARS));
    }
    w.bool(signing);
    if let Some(title) = title.filter(|_| signing) {
        w.string(cut(title, MAX_TITLE_CHARS));
    }
    Ok(w.into_inner())
}

#[derive(Default)]
pub struct V1_17_1To1_17 {
    /// The state number the server last sent for each open window.
    states: HashMap<i8, i32>,
}

impl V1_17_1To1_17 {
    pub fn new() -> Self {
        Self::default()
    }

    fn play_clientbound(&mut self, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        match pkt.id {
            REMOVE_ENTITIES => {
                for id in removed_entities(p).map_err(e)? {
                    let mut w = Writer::new();
                    w.varint(id);
                    out.push(Packet::new(REMOVE_ENTITIES, w.into_inner()));
                }
            }
            CONTAINER_CLOSE => {
                if let Some(window) = p.first() {
                    self.states.remove(&(*window as i8));
                }
                out.push(pkt.clone());
            }
            CONTAINER_SET_SLOT => {
                let (window, state, payload) = container_set_slot(p).map_err(e)?;
                self.states.insert(window, state);
                out.push(Packet::new(pkt.id, payload));
            }
            CONTAINER_SET_CONTENT => {
                let (window, state, payload, cursor) = container_set_content(p).map_err(e)?;
                self.states.insert(window, state);
                out.push(Packet::new(pkt.id, payload));
                out.push(Packet::new(CONTAINER_SET_SLOT, cursor));
            }
            _ => out.push(Packet::new(pkt.id, std::mem::take(&mut pkt.payload))),
        }
        Ok(())
    }

    fn play_serverbound(&mut self, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: DecodeError| format!("serverbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            CONTAINER_CLICK => {
                // each number is good for one click; the server sends a new one after it
                let state = p.first().and_then(|window| self.states.remove(&(*window as i8))).unwrap_or(0);
                container_click(p, state).map_err(e)?
            }
            CONTAINER_CLOSE_S => {
                if let Some(window) = p.first() {
                    self.states.remove(&(*window as i8));
                }
                p.to_vec()
            }
            EDIT_BOOK => edit_book(p).map_err(e)?,
            _ => std::mem::take(&mut pkt.payload),
        };
        out.push(Packet::new(pkt.id, payload));
        Ok(())
    }
}

impl Translator for V1_17_1To1_17 {
    fn from(&self) -> Version {
        Version { protocol: 756 }
    }
    fn to(&self) -> Version {
        Version { protocol: 755 }
    }
    fn clientbound(&mut self, state: State, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            State::Configuration => {
                out.push(Packet::new(pkt.id, std::mem::take(&mut pkt.payload)));
                Ok(())
            }
        }
    }
    fn serverbound(&mut self, state: State, pkt: &mut Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_serverbound(pkt, out),
            State::Configuration => {
                out.push(Packet::new(pkt.id, std::mem::take(&mut pkt.payload)));
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(w: &mut Writer, id: i32) {
        w.bool(true).varint(id).i8(3).u8(0); // no data
    }

    fn play(step: &mut V1_17_1To1_17, clientbound: bool, id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        let pkt = Packet::new(id, payload);
        if clientbound { step.clientbound(State::Play, &mut pkt.clone(), &mut out) } else { step.serverbound(State::Play, &mut pkt.clone(), &mut out) }.unwrap();
        out
    }

    #[test]
    fn entities_are_removed_one_packet_each() {
        let mut step = V1_17_1To1_17::new();
        let mut w = Writer::new();
        w.varint(3).varint(7).varint(300).varint(9);
        let out = play(&mut step, true, REMOVE_ENTITIES, w.into_inner());
        assert_eq!(out.iter().map(|p| (p.id, p.payload.clone())).collect::<Vec<_>>(), [(58, vec![7]), (58, vec![0xac, 0x02]), (58, vec![9])]);
        assert!(removed_entities(&[0xff, 0xff, 0xff, 0xff, 0x07]).is_err());
    }

    #[test]
    fn the_inventory_state_is_kept_here_and_added_to_the_next_click() {
        let mut step = V1_17_1To1_17::new();
        // the whole inventory: window 2, state 41, two slots, a diamond on the cursor
        let mut w = Writer::new();
        w.u8(2).varint(41).varint(2);
        stack(&mut w, 5);
        w.bool(false);
        stack(&mut w, 700);
        let out = play(&mut step, true, CONTAINER_SET_CONTENT, w.into_inner());
        assert_eq!(out.len(), 2);
        let mut want = Writer::new();
        want.u8(2).i16(2);
        stack(&mut want, 5);
        want.bool(false);
        assert_eq!((out[0].id, &out[0].payload), (CONTAINER_SET_CONTENT, &want.buf));
        let mut cursor = Writer::new();
        cursor.i8(-1).i16(-1);
        stack(&mut cursor, 700);
        assert_eq!((out[1].id, &out[1].payload), (CONTAINER_SET_SLOT, &cursor.buf));
        // a click in that window carries 41; the one after it has no number until the server sends one
        let click = vec![2u8, 0, 5, 0, 0, 0, 0];
        assert_eq!(play(&mut step, false, CONTAINER_CLICK, click.clone())[0].payload, [2, 41, 0, 5, 0, 0, 0, 0]);
        assert_eq!(play(&mut step, false, CONTAINER_CLICK, click.clone())[0].payload, [2, 0, 0, 5, 0, 0, 0, 0]);
        // one slot: window 2, state 44, slot 3
        let mut w = Writer::new();
        w.i8(2).varint(44).i16(3);
        stack(&mut w, 5);
        let out = play(&mut step, true, CONTAINER_SET_SLOT, w.into_inner());
        assert_eq!(&out[0].payload[..3], [2, 0, 3]);
        assert_eq!(play(&mut step, false, CONTAINER_CLICK, click)[0].payload[1], 44);
    }

    #[test]
    fn a_book_item_becomes_its_pages() {
        let mut step = V1_17_1To1_17::new();
        let mut book = Tag::compound();
        book.insert("pages", Tag::List(vec![Tag::String("one".into()), Tag::String("two".into())]));
        book.insert("title", Tag::String("Diary".into()));
        let edit = |signing: bool, with_book: bool| {
            let mut w = Writer::new();
            w.bool(with_book);
            if with_book {
                w.varint(900).i8(1);
                nbt::write_named(&mut w, &book);
            }
            w.bool(signing).varint(4);
            w.into_inner()
        };
        let mut want = Writer::new();
        want.varint(4).varint(2).string("one").string("two").bool(true).string("Diary");
        assert_eq!(play(&mut step, false, EDIT_BOOK, edit(true, true))[0].payload, want.buf);
        let mut want = Writer::new();
        want.varint(4).varint(2).string("one").string("two").bool(false);
        assert_eq!(play(&mut step, false, EDIT_BOOK, edit(false, true))[0].payload, want.buf);
        assert_eq!(play(&mut step, false, EDIT_BOOK, edit(false, false))[0].payload, [4, 0, 0]);
    }

    #[test]
    fn everything_else_passes_through() {
        let mut step = V1_17_1To1_17::new();
        assert_eq!(play(&mut step, true, 0x30, vec![1, 2, 3])[0].payload, [1, 2, 3]);
        assert_eq!(play(&mut step, false, 0x03, vec![4])[0].payload, [4]);
        assert!(step.clientbound(State::Play, &mut Packet::new(CONTAINER_SET_CONTENT, vec![1]), &mut Vec::new()).is_err());
    }
}
