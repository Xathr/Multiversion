//! 1.18.2 (protocol 758) ↔ 1.18 / 1.18.1 (protocol 757).
//!
//! A small step: both versions have the same packets with the same ids, and the same blocks,
//! items and entities. What 1.18.2 changed:
//! - Status effects are named by a full number instead of one byte.
//! - A dimension type's "infiniburn" names a block tag with a leading `#`.
//! - Commands have two new argument types, which take any id from a registry ("resource")
//!   or an id or tag ("resource_or_tag"); 1.18 gets a plain string for them.

use super::{Packet, State, TResult, Translator};
use crate::proto::buf::{DecodeError, Reader, Result, Writer};
use crate::proto::nbt::{self, Tag};
use crate::proto::version::Version;

// The clientbound play packets this step changes (the same ids in both versions).
const COMMANDS: i32 = 18;
const LOGIN: i32 = 38;
const REMOVE_MOB_EFFECT: i32 = 59;
const RESPAWN: i32 = 61;
const UPDATE_MOB_EFFECT: i32 = 101;

fn err(msg: String) -> DecodeError {
    DecodeError::Other(msg)
}

/// "#minecraft:infiniburn_overworld" → "minecraft:infiniburn_overworld".
fn strip_tag_prefix(dimension_type: &mut Tag) {
    if let Some(Tag::String(tag)) = dimension_type.get_mut("infiniburn")
        && let Some(plain) = tag.strip_prefix('#')
    {
        *tag = plain.to_string();
    }
}

/// LOGIN: every dimension type in the registry data, and the one the player is in.
fn login(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    r.copy_to(&mut w, |r| {
        r.i32()?; // entity id
        r.bool()?; // hardcore
        r.u8()?; // game mode
        r.i8()?; // previous game mode
        r.list(|r| r.string().map(|_| ()))?; // worlds
        Ok(())
    })?;
    let mut registries = nbt::read_named_optional(&mut r)?.ok_or_else(|| err("login has no registries".into()))?;
    let types = registries.get_mut("minecraft:dimension_type").and_then(|registry| registry.get_mut("value")).and_then(Tag::as_list_mut);
    for entry in types.into_iter().flatten() {
        if let Some(element) = entry.get_mut("element") {
            strip_tag_prefix(element);
        }
    }
    nbt::write_named(&mut w, &registries);
    let mut dimension = nbt::read_named_optional(&mut r)?.ok_or_else(|| err("login has no dimension type".into()))?;
    strip_tag_prefix(&mut dimension);
    nbt::write_named(&mut w, &dimension);
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// RESPAWN starts with the dimension type.
fn respawn(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    let mut dimension = nbt::read_named_optional(&mut r)?.ok_or_else(|| err("respawn has no dimension type".into()))?;
    strip_tag_prefix(&mut dimension);
    nbt::write_named(&mut w, &dimension);
    w.raw(r.remaining());
    Ok(w.into_inner())
}

/// UPDATE_MOB_EFFECT / REMOVE_MOB_EFFECT: entity id, then the effect, which 1.18 reads as one
/// byte. `None` for an effect whose id doesn't fit (no vanilla effect).
fn mob_effect(p: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    w.varint(r.varint()?);
    let Ok(effect) = i8::try_from(r.varint()?) else { return Ok(None) };
    w.i8(effect);
    w.raw(r.remaining());
    Ok(Some(w.into_inner()))
}

/// A number argument's limits: a flags byte, then the minimum and maximum, each only when its
/// flag is set.
fn number_range(r: &mut Reader, w: &mut Writer, size: usize) -> Result<()> {
    let present = r.u8()?;
    w.u8(present);
    w.raw(r.bytes(size * usize::from(present & 1) + size * usize::from((present >> 1) & 1))?);
    Ok(())
}

/// COMMANDS: the two registry argument types become a string that may be quoted. Every other
/// argument is copied; only the ones that carry properties need reading.
fn commands(p: &[u8]) -> Result<Vec<u8>> {
    let mut r = Reader::new(p);
    let mut w = Writer::with_capacity(p.len());
    let nodes = r.varint()?;
    w.varint(nodes);
    for _ in 0..nodes.max(0) {
        let flags = r.u8()?;
        w.u8(flags);
        let children = r.varint()?;
        w.varint(children);
        for _ in 0..children.max(0) {
            w.varint(r.varint()?);
        }
        if flags & 0x08 != 0 {
            w.varint(r.varint()?); // redirect
        }
        let node = flags & 0x03;
        if node == 1 || node == 2 {
            w.string(r.string()?); // name
        }
        if node != 2 {
            continue;
        }
        let parser = r.string()?;
        match parser {
            "minecraft:resource" | "minecraft:resource_or_tag" => {
                r.string()?; // registry
                w.string("brigadier:string").varint(1); // quotable phrase
            }
            _ => {
                w.string(parser);
                match parser {
                    "brigadier:float" | "brigadier:integer" => number_range(&mut r, &mut w, 4)?,
                    "brigadier:double" | "brigadier:long" => number_range(&mut r, &mut w, 8)?,
                    "brigadier:string" => {
                        w.varint(r.varint()?);
                    }
                    "minecraft:entity" | "minecraft:score_holder" => {
                        w.u8(r.u8()?);
                    }
                    "minecraft:range" => {
                        w.u8(r.u8()?); // decimals allowed
                    }
                    _ => {}
                }
            }
        }
        if flags & 0x10 != 0 {
            w.string(r.string()?); // suggestions
        }
    }
    w.varint(r.varint()?); // root
    Ok(w.into_inner())
}

#[derive(Default)]
pub struct V1_18_2To1_18;

impl V1_18_2To1_18 {
    pub fn new() -> Self {
        Self
    }

    fn play_clientbound(&mut self, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        let p = &pkt.payload[..];
        let e = |e: DecodeError| format!("clientbound 0x{:02x}: {e}", pkt.id);
        let payload = match pkt.id {
            LOGIN => login(p).map_err(e)?,
            RESPAWN => respawn(p).map_err(e)?,
            COMMANDS => commands(p).map_err(e)?,
            UPDATE_MOB_EFFECT | REMOVE_MOB_EFFECT => match mob_effect(p).map_err(e)? {
                Some(v) => v,
                None => return Ok(()),
            },
            _ => p.to_vec(),
        };
        out.push(Packet::new(pkt.id, payload));
        Ok(())
    }
}

impl Translator for V1_18_2To1_18 {
    fn from(&self) -> Version {
        Version { protocol: 758 }
    }
    fn to(&self) -> Version {
        Version { protocol: 757 }
    }
    fn clientbound(&mut self, state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        match state {
            State::Play => self.play_clientbound(pkt, out),
            // neither version has a configuration phase: a newer step stands in for it
            State::Configuration => {
                out.push(pkt.clone());
                Ok(())
            }
        }
    }
    /// Everything the client sends is the same in both versions.
    fn serverbound(&mut self, _state: State, pkt: &Packet, out: &mut Vec<Packet>) -> TResult {
        out.push(pkt.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cb(id: i32, payload: Vec<u8>) -> Vec<Packet> {
        let mut out = Vec::new();
        V1_18_2To1_18::new().clientbound(State::Play, &Packet::new(id, payload), &mut out).unwrap();
        out
    }

    fn dimension(infiniburn: &str) -> Tag {
        Tag::Compound(vec![("height".into(), Tag::Int(384)), ("infiniburn".into(), Tag::String(infiniburn.into()))])
    }

    fn registries(infiniburn: &str) -> Tag {
        let entry = Tag::Compound(vec![("name".into(), Tag::String("minecraft:overworld".into())), ("id".into(), Tag::Int(0)), ("element".into(), dimension(infiniburn))]);
        Tag::Compound(vec![("minecraft:dimension_type".into(), Tag::Compound(vec![("type".into(), Tag::String("minecraft:dimension_type".into())), ("value".into(), Tag::List(vec![entry]))]))])
    }

    fn login_packet(infiniburn: &str) -> Vec<u8> {
        let mut w = Writer::new();
        w.i32(42).bool(false).u8(1).i8(-1).varint(1).string("minecraft:overworld");
        nbt::write_named(&mut w, &registries(infiniburn));
        nbt::write_named(&mut w, &dimension(infiniburn));
        w.string("minecraft:overworld").i64(99).varint(20).varint(10).varint(8).bool(false).bool(true).bool(false).bool(true);
        w.into_inner()
    }

    #[test]
    fn versions() {
        let t = V1_18_2To1_18::new();
        assert_eq!((t.from().protocol, t.to().protocol), (758, 757));
        assert!(!t.emulates_configuration());
    }

    #[test]
    fn dimension_types_name_infiniburn_without_the_tag_mark() {
        let out = cb(LOGIN, login_packet("#minecraft:infiniburn_overworld"));
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].id, &out[0].payload), (LOGIN, &login_packet("minecraft:infiniburn_overworld")));

        let respawn_packet = |infiniburn: &str| {
            let mut w = Writer::new();
            nbt::write_named(&mut w, &dimension(infiniburn));
            w.string("minecraft:overworld").i64(7).u8(0).i8(1).bool(false).bool(false).bool(true);
            w.into_inner()
        };
        assert_eq!(cb(RESPAWN, respawn_packet("#minecraft:infiniburn_end"))[0].payload, respawn_packet("minecraft:infiniburn_end"));
        // already plain: unchanged
        assert_eq!(cb(RESPAWN, respawn_packet("minecraft:infiniburn_end"))[0].payload, respawn_packet("minecraft:infiniburn_end"));
        let mut out = Vec::new();
        assert!(V1_18_2To1_18::new().clientbound(State::Play, &Packet::new(LOGIN, vec![0, 0, 0, 1]), &mut out).is_err());
    }

    #[test]
    fn effects_are_one_byte() {
        let mut w = Writer::new();
        w.varint(300).varint(33).i8(1).varint(600).u8(3);
        let mut want = Writer::new();
        want.varint(300).i8(33).i8(1).varint(600).u8(3);
        assert_eq!(cb(UPDATE_MOB_EFFECT, w.into_inner())[0].payload, want.into_inner());
        assert_eq!(cb(REMOVE_MOB_EFFECT, vec![5, 15])[0].payload, [5, 15]);
        // an id that doesn't fit one byte can't be sent at all
        let mut w = Writer::new();
        w.varint(5).varint(200);
        assert!(cb(REMOVE_MOB_EFFECT, w.into_inner()).is_empty());
    }

    #[test]
    fn registry_arguments_become_strings() {
        let mut w = Writer::new();
        w.varint(5);
        w.u8(0).varint(4).varint(1).varint(2).varint(3).varint(4);
        w.u8(2).varint(0).string("biome").string("minecraft:resource_or_tag").string("minecraft:worldgen/biome");
        w.u8(2 | 0x10).varint(0).string("structure").string("minecraft:resource").string("minecraft:worldgen/structure").string("minecraft:ask_server");
        w.u8(2).varint(0).string("count").string("brigadier:integer").u8(1).i32(1);
        w.u8(2).varint(0).string("who").string("minecraft:entity").u8(2);
        w.varint(0);
        let mut want = Writer::new();
        want.varint(5);
        want.u8(0).varint(4).varint(1).varint(2).varint(3).varint(4);
        want.u8(2).varint(0).string("biome").string("brigadier:string").varint(1);
        want.u8(2 | 0x10).varint(0).string("structure").string("brigadier:string").varint(1).string("minecraft:ask_server");
        want.u8(2).varint(0).string("count").string("brigadier:integer").u8(1).i32(1);
        want.u8(2).varint(0).string("who").string("minecraft:entity").u8(2);
        want.varint(0);
        assert_eq!(cb(COMMANDS, w.into_inner())[0].payload, want.into_inner());
    }

    #[test]
    fn everything_else_passes_through() {
        assert_eq!(cb(33, vec![1, 2, 3])[0].payload, [1, 2, 3]);
        let mut t = V1_18_2To1_18::new();
        let mut out = Vec::new();
        t.serverbound(State::Play, &Packet::new(3, vec![4, 5]), &mut out).unwrap();
        t.clientbound(State::Configuration, &Packet::new(LOGIN, vec![6]), &mut out).unwrap();
        assert_eq!(out.iter().map(|p| (p.id, p.payload.clone())).collect::<Vec<_>>(), [(3, vec![4, 5]), (LOGIN, vec![6])]);
    }

    /// The ids this step names are the 1.18.2 ones.
    #[test]
    fn packet_ids_match_the_1_18_list() {
        use crate::translate::v1_18_2::ids::old;
        assert_eq!((COMMANDS, LOGIN, REMOVE_MOB_EFFECT, RESPAWN, UPDATE_MOB_EFFECT), (old::COMMANDS, old::LOGIN, old::REMOVE_MOB_EFFECT, old::RESPAWN, old::UPDATE_MOB_EFFECT));
    }
}
