//! Checks Pumpkin's side of the setup when the server starts: the plugin begins a login to
//! Pumpkin the way it does for a player, sees how Pumpkin answers, and hangs up before the login
//! completes. The console then says the setup is right, or which pumpkin.toml line is not.

use crate::auth::Profile;
use crate::config::Need;
use crate::conn::{ids, velocity_answer};
use crate::net::reactor::Token;
use crate::proto::buf::{offline_uuid, Reader, Writer};
use crate::proto::frame::{Inbound, Outbound};
use crate::proto::version::BACKEND;
use crate::translate::Packet;

/// The name the test login uses. No player is created: the login is dropped before it finishes.
const NAME: &str = "MV_SetupCheck";
/// How often to try reaching Pumpkin (it opens its port a moment after plugins load).
pub const MAX_ATTEMPTS: u32 = 15;
pub const RETRY_MS: u64 = 2000;

pub enum Verdict {
    /// Pumpkin is where the plugin expects it and accepts forwarded players.
    Ready,
    /// These pumpkin.toml lines are not right yet.
    Needs(Vec<Need>),
    /// Pumpkin refused the test login for a reason of its own.
    Unclear(String),
}

pub struct Check {
    pub token: Option<Token>,
    pub attempts: u32,
    pub next_try_ms: u64,
    pub done: bool,
    inbound: Inbound,
    outbound: Outbound,
    forwarded: bool,
}

impl Default for Check {
    fn default() -> Self {
        Self { token: None, attempts: 0, next_try_ms: 0, done: false, inbound: Inbound::default(), outbound: Outbound::default(), forwarded: false }
    }
}

impl Check {
    /// Forgets a connection that ended, ready for another try.
    pub fn reset_connection(&mut self) {
        self.token = None;
        self.inbound = Inbound::default();
        self.outbound = Outbound::default();
        self.forwarded = false;
    }

    fn frame(&mut self, pkt: &Packet, out: &mut Vec<u8>) {
        self.outbound.frame(&pkt.encode(), out);
    }

    /// Handshake and login start, sent once connected.
    pub fn hello(&mut self, backend: std::net::SocketAddr) -> Vec<u8> {
        let mut out = Vec::new();
        let mut w = Writer::new();
        w.varint(BACKEND.protocol);
        w.string(&backend.ip().to_string());
        w.u16(backend.port());
        w.varint(2);
        self.frame(&Packet::new(0, w.into_inner()), &mut out);
        let mut w = Writer::new();
        w.string(NAME);
        w.uuid(offline_uuid(NAME));
        self.frame(&Packet::new(ids::LOGIN_S_HELLO, w.into_inner()), &mut out);
        out
    }

    /// Pumpkin's reply. Returns what to send back and, once it is known, the verdict.
    pub fn on_data(&mut self, data: &[u8], secret: &str) -> (Vec<u8>, Option<Verdict>) {
        let not_forwarding = || Verdict::Needs(vec![Need::ProxyEnabled, Need::VelocityEnabled, Need::Secret]);
        let mut out = Vec::new();
        self.inbound.feed(data);
        loop {
            let raw = match self.inbound.next_packet() {
                Ok(Some(raw)) => raw,
                Ok(None) => return (out, None),
                Err(e) => return (out, Some(Verdict::Unclear(e))),
            };
            let mut r = Reader::new(&raw);
            let Ok(id) = r.varint() else { return (out, Some(Verdict::Unclear("an unreadable packet".into()))) };
            match id {
                ids::LOGIN_C_COMPRESSION => {
                    let threshold = r.varint().unwrap_or(-1);
                    self.inbound.compression = threshold;
                    self.outbound.compression = threshold;
                }
                ids::LOGIN_C_CUSTOM_QUERY => {
                    let message_id = r.varint().unwrap_or(0);
                    let answer = if r.string().is_ok_and(|channel| channel == ids::VELOCITY_CHANNEL) {
                        self.forwarded = true;
                        let profile = Profile { id: offline_uuid(NAME), name: NAME.into(), properties: Vec::new() };
                        velocity_answer(message_id, "127.0.0.1", &profile, secret)
                    } else {
                        let mut w = Writer::new();
                        w.varint(message_id);
                        w.bool(false);
                        Packet::new(ids::LOGIN_S_CUSTOM_QUERY_ANSWER, w.into_inner())
                    };
                    self.frame(&answer, &mut out);
                }
                // Pumpkin wants to check the player with Mojang itself: forwarding is off.
                ids::LOGIN_C_HELLO => return (out, Some(not_forwarding())),
                ids::LOGIN_C_FINISHED => {
                    // Accepted. Without having asked for the forwarded identity, Pumpkin is
                    // letting anyone in under any name.
                    return (out, Some(if self.forwarded { Verdict::Ready } else { not_forwarding() }));
                }
                ids::LOGIN_C_DISCONNECT => {
                    let reason = r.string().unwrap_or("").to_string();
                    return (
                        out,
                        Some(if reason.contains("verify") {
                            Verdict::Needs(vec![Need::Secret])
                        } else if self.forwarded {
                            // The identity was accepted; the refusal is about this made-up player (a whitelist, say).
                            Verdict::Ready
                        } else {
                            Verdict::Unclear(crate::color::clean(&reason, 200))
                        }),
                    );
                }
                _ => {}
            }
        }
    }
}
