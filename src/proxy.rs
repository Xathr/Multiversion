//! The proxy loop: owns the reactor, maps sockets to connections, runs session checks.

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::auth::{self, PendingAuth};
use crate::config::Config;
use crate::conn::{Action, Connection, Phase, Shared};
use crate::crypto::ServerKeys;
use crate::log;
use crate::net::reactor::{Event, Handler, Reactor, Token};
use crate::platform::now_millis;

static STOP: AtomicBool = AtomicBool::new(false);

pub fn request_stop() {
    STOP.store(true, Ordering::Relaxed);
    if let Some(proxy) = PROXY.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
        proxy.reactor.stop();
    }
}

fn resolve(addr: &str) -> Result<SocketAddr, String> {
    addr.to_socket_addrs()
        .map_err(|e| format!("cannot resolve {addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address for {addr}"))
}

struct ProxyHandler {
    config: Config,
    keys: ServerKeys,
    backend_addr: SocketAddr,
    conns: HashMap<Token, Connection>,
    /// backend token -> client token
    backend_of: HashMap<Token, Token>,
    /// external pollable id -> (client token, pending auth)
    auths: HashMap<u32, (Token, PendingAuth)>,
    /// Pumpkin's last server-list reply and when it arrived (ms).
    status_cache: Option<(u64, String)>,
    /// The update check or download in progress, and the id the reactor reports it under.
    update: Option<(u32, crate::updater::Flow)>,
    /// New connections per address, for limits.max_connections_per_ip_per_10s.
    rate: crate::ratelimit::ConnectionRate,
}

/// How long a server-list reply from Pumpkin is reused.
const STATUS_CACHE_MS: u64 = 1000;

fn fresh_status(cache: &Option<(u64, String)>) -> Option<&str> {
    cache.as_ref().filter(|(at, _)| now_millis().saturating_sub(*at) < STATUS_CACHE_MS).map(|(_, json)| json.as_str())
}

impl ProxyHandler {
    fn apply(&mut self, reactor: &mut Reactor, client: Token, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::SendClient(bytes) => reactor.send(client, &bytes),
                Action::SendBackend(bytes) => {
                    if let Some(b) = self.conns.get(&client).and_then(|c| c.backend) {
                        reactor.send(b, &bytes);
                    }
                }
                Action::ConnectBackend => match reactor.connect(self.backend_addr) {
                    Ok(token) => {
                        if let Some(c) = self.conns.get_mut(&client) {
                            c.backend = Some(token);
                        }
                        self.backend_of.insert(token, client);
                    }
                    Err(e) => {
                        let mut more = Vec::new();
                        if let Some(c) = self.conns.get_mut(&client) {
                            let shared = Shared { config: &self.config, keys: &self.keys, backend_addr: self.backend_addr, cached_status: fresh_status(&self.status_cache) };
                            c.on_backend_connect_failed(&e, &shared, &mut more);
                        }
                        self.apply(reactor, client, more);
                    }
                },
                Action::StartAuth { username, server_hash, ip } => {
                    match auth::start(&self.config.auth.session_server, &username, &server_hash, ip.as_deref()) {
                        Ok(pending) => {
                            let id = reactor.watch(pending.pollable());
                            self.auths.insert(id, (client, pending));
                        }
                        Err(e) => {
                            let mut more = Vec::new();
                            if let Some(c) = self.conns.get_mut(&client) {
                                let shared = Shared { config: &self.config, keys: &self.keys, backend_addr: self.backend_addr, cached_status: fresh_status(&self.status_cache) };
                                c.on_auth(auth::AuthOutcome::Error(e), &shared, &mut more);
                            }
                            self.apply(reactor, client, more);
                        }
                    }
                }
                Action::Close(reason) => {
                    self.close(reactor, client, &reason, false);
                }
                Action::CacheStatus(json) => {
                    self.status_cache = Some((now_millis(), json));
                }
                Action::CloseBackend => {
                    if let Some(tok) = self.conns.get_mut(&client).and_then(|c| c.backend.take()) {
                        reactor.close_gracefully(tok);
                        self.backend_of.remove(&tok);
                    }
                }
                Action::Log(msg) => log::info(&msg),
            }
        }
    }

    /// Keeps an update job going: waits for its next step, or logs the outcome.
    fn drive_update(&mut self, reactor: &mut Reactor, progress: crate::updater::FlowProgress) {
        match progress {
            crate::updater::FlowProgress::Pending(flow) => {
                let id = reactor.watch(flow.pollable());
                self.update = Some((id, flow));
            }
            crate::updater::FlowProgress::Done(lines) => {
                for line in lines {
                    log::info(&line);
                }
            }
        }
    }

    fn close(&mut self, reactor: &mut Reactor, client: Token, reason: &str, abrupt: bool) {
        if let Some(c) = self.conns.remove(&client) {
            if crate::config::get().logging.leaves && c.phase != Phase::Status && c.phase != Phase::StatusBackendHandshake && c.phase != Phase::StatusBackendWait && !reason.starts_with("status") {
                let msg = format!("{} {} {reason}", c.describe(), crate::color::warn("disconnected:"));
                if c.phase == Phase::Handshake && c.username.is_empty() {
                    // Never sent a handshake: a port scanner or a client's connection probe.
                    log::debug(&msg);
                } else {
                    log::info(&msg);
                }
            }
            if abrupt {
                reactor.close_now(client);
            } else {
                reactor.close_gracefully(client);
            }
            if let Some(b) = c.backend {
                self.backend_of.remove(&b);
                reactor.close_gracefully(b);
            }
        }
    }
}

impl Handler for ProxyHandler {
    fn handle(&mut self, reactor: &mut Reactor, event: Event) {
        if STOP.load(Ordering::Relaxed) {
            reactor.stop();
            return;
        }
        let _ = &reactor;
        match event {
            Event::Accepted { token, peer } => {
                if self.conns.len() >= self.config.limits.max_connections {
                    if self.config.logging.debug {
                        log::info(&format!("{} {peer} refused: limits.max_connections ({}) reached", crate::color::dim("debug:"), self.config.limits.max_connections));
                    }
                    reactor.close_now(token);
                    return;
                }
                if !self.rate.allow(peer.ip(), now_millis(), self.config.limits.max_connections_per_ip_per_10s) {
                    if self.config.logging.debug {
                        log::info(&format!("{} {peer} refused: limits.max_connections_per_ip_per_10s ({}) reached", crate::color::dim("debug:"), self.config.limits.max_connections_per_ip_per_10s));
                    }
                    reactor.close_now(token);
                    return;
                }
                let logging_in = self.conns.values().filter(|c| c.peer.ip() == peer.ip() && c.is_logging_in()).count();
                if !self.config.limits.allows_login_from(logging_in) {
                    if self.config.logging.debug {
                        log::info(&format!("{} {peer} refused: limits.max_logins_per_ip ({}) reached", crate::color::dim("debug:"), self.config.limits.max_logins_per_ip));
                    }
                    reactor.close_now(token);
                    return;
                }
                if self.config.logging.debug {
                    log::info(&format!("{} {peer} connected", crate::color::dim("debug:")));
                }
                self.conns.insert(token, Connection::new(token, peer));
            }
            Event::Data { token, data } => {
                let mut actions = Vec::new();
                if let Some(client) = self.backend_of.get(&token).copied() {
                    if let Some(c) = self.conns.get_mut(&client) {
                        let shared = Shared { config: &self.config, keys: &self.keys, backend_addr: self.backend_addr, cached_status: fresh_status(&self.status_cache) };
                        c.on_backend_data(&data, &shared, &mut actions);
                    }
                    self.apply(reactor, client, actions);
                } else if let Some(c) = self.conns.get_mut(&token) {
                    let shared = Shared { config: &self.config, keys: &self.keys, backend_addr: self.backend_addr, cached_status: fresh_status(&self.status_cache) };
                    c.on_client_data(&data, &shared, &mut actions);
                    self.apply(reactor, token, actions);
                }
            }
            Event::Connected { token, result } => {
                let Some(client) = self.backend_of.get(&token).copied() else { return };
                let mut actions = Vec::new();
                if let Some(c) = self.conns.get_mut(&client) {
                    let shared = Shared { config: &self.config, keys: &self.keys, backend_addr: self.backend_addr, cached_status: fresh_status(&self.status_cache) };
                    match result {
                        Ok(()) => c.on_backend_connected(&shared, &mut actions),
                        Err(e) => c.on_backend_connect_failed(&e, &shared, &mut actions),
                    }
                }
                self.apply(reactor, client, actions);
            }
            Event::Closed { token, reason } => {
                if token == 0 {
                    log::warn(&format!("listener: {reason}"));
                    return;
                }
                if let Some(client) = self.backend_of.remove(&token) {
                    let mut actions = Vec::new();
                    if let Some(c) = self.conns.get_mut(&client) {
                        c.backend = None;
                        let shared = Shared { config: &self.config, keys: &self.keys, backend_addr: self.backend_addr, cached_status: fresh_status(&self.status_cache) };
                        c.on_backend_closed(&reason, &shared, &mut actions);
                    }
                    self.apply(reactor, client, actions);
                } else if self.conns.contains_key(&token) {
                    self.close(reactor, token, &reason, true);
                }
            }
            Event::External { id } => {
                if self.update.as_ref().is_some_and(|(update_id, _)| *update_id == id) {
                    let (_, flow) = self.update.take().expect("checked above");
                    self.drive_update(reactor, flow.advance());
                    return;
                }
                if let Some((client, pending)) = self.auths.remove(&id) {
                    let outcome = match auth::advance(pending) {
                        auth::Progress::Done(outcome) => outcome,
                        auth::Progress::Pending(pending) => {
                            // Headers in, body still arriving: wait for more without blocking.
                            let id = reactor.watch(pending.pollable());
                            self.auths.insert(id, (client, pending));
                            return;
                        }
                    };
                    let mut actions = Vec::new();
                    if let Some(c) = self.conns.get_mut(&client) {
                        let shared = Shared { config: &self.config, keys: &self.keys, backend_addr: self.backend_addr, cached_status: fresh_status(&self.status_cache) };
                        c.on_auth(outcome, &shared, &mut actions);
                    }
                    self.apply(reactor, client, actions);
                }
            }
            Event::Tick => {
                // Drop connections stuck before play for too long.
                // An update check or download asked for with /multiversion update.
                if self.update.is_none()
                    && let Some(job) = crate::updater::take_request()
                {
                    self.drive_update(reactor, crate::updater::Flow::start(job));
                }
                let now = now_millis();
                self.rate.prune(now);
                let timeout = self.config.limits.login_timeout_secs * 1000;
                let stale: Vec<Token> = self
                    .conns
                    .iter()
                    .filter(|(_, c)| c.phase != Phase::Play && c.phase != Phase::Configuration && now.saturating_sub(c.created_ms) > timeout)
                    .map(|(t, _)| *t)
                    .collect();
                for t in stale {
                    self.close(reactor, t, "login timeout", true);
                }
            }
        }
    }
}

struct Proxy {
    reactor: Reactor,
    handler: ProxyHandler,
}

// The plugin is single-threaded WebAssembly; the host never runs two of our callbacks at once.
unsafe impl Send for Proxy {}

static PROXY: std::sync::Mutex<Option<Proxy>> = std::sync::Mutex::new(None);

/// Pumpkin runs this plugin under its sequential policy: while one of our callbacks runs, no other
/// (unload, events) can. So the proxy loop runs in slices of at most this many milliseconds, one
/// per server tick, and returns control to the host in between.
pub const SLICE_MS: u64 = 45;

/// Runs one slice of the proxy loop. Called from a repeating scheduled task every tick.
pub fn tick() {
    let mut guard = PROXY.lock().unwrap_or_else(|p| p.into_inner());
    let Some(proxy) = guard.as_mut() else { return };
    proxy.reactor.run_for(&mut proxy.handler, SLICE_MS);
    if proxy.reactor.stopped() {
        log::info(&crate::color::warn("proxy stopped"));
        *guard = None;
    }
}

/// Sets the proxy up (keys, listener). After this, `tick` drives it.
pub fn start(config: Config) -> Result<(), String> {
    let listen = resolve(&config.listen.address)?;
    let backend_addr = resolve(&config.backend.address)?;
    if !backend_addr.ip().is_loopback() {
        log::warn(&format!(
            "backend.address {backend_addr} is not a loopback address: other machines can reach Pumpkin's port directly, \
             and the forwarding secret is all that stops them logging in as anyone. Use 127.0.0.1, or firewall that port."
        ));
    }
    let keys = ServerKeys::generate()?;
    let mut reactor = Reactor::new();
    if let Err(e) = reactor.listen(listen, 128) {
        if e.contains("address-in-use") || e.contains("AddressInUse") {
            return Err(format!(
                "port {} is taken, most likely by Pumpkin itself. Set [networking.java] address = \"{}\" in pumpkin.toml \
                 (all the settings the plugin needs are in plugins/data/multiversion/pumpkin-settings.txt) and restart.",
                listen.port(),
                config.backend.address
            ));
        }
        return Err(e);
    }
    log::info(&format!("{} on {}", crate::color::good("accepting connections"), crate::color::version(&listen.to_string())));
    let handler = ProxyHandler {
        config,
        keys,
        backend_addr,
        conns: HashMap::new(),
        backend_of: HashMap::new(),
        auths: HashMap::new(),
        status_cache: None,
        rate: crate::ratelimit::ConnectionRate::default(),
        update: None,
    };
    *PROXY.lock().unwrap_or_else(|p| p.into_inner()) = Some(Proxy { reactor, handler });
    Ok(())
}
