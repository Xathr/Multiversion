//! Multiversion's own numbers, for a performance monitor to ask for.
//!
//! Pumpkin gives a plugin no way to measure another, so Multiversion measures itself: how long its
//! code has run in total, how often, and the longest single run. Another plugin (perfmon) can
//! ask for them with a message, `{"perfmon":1,"ask":"report"}`, and from a series of answers
//! tell whether Multiversion is using much CPU, spiking or leaking memory.
//!
//! Multiversion's work is its proxy loop, which Pumpkin runs beside the server tick and which
//! spends most of each turn waiting for network data. Only the time it is busy is counted, and
//! the report says the work is off the tick (`"off_tick":true`): it uses CPU, it does not make
//! ticks longer.

use std::cell::Cell;
use std::time::Instant;

thread_local! {
    /// Microseconds of work since load, runs counted, and the longest run since the last report.
    static WORK: Cell<(u64, u64, u64)> = const { Cell::new((0, 0, 0)) };
    /// How many timed runs are in progress, one inside the other; only the outermost is counted.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Times one run of Multiversion's code, from where it is made to where it goes out of scope. Put one
/// at the top of everything Pumpkin calls: the tick task, event handlers, commands.
pub struct Timing(Option<Instant>);

impl Timing {
    pub fn start() -> Self {
        let depth = DEPTH.with(|d| {
            d.set(d.get() + 1);
            d.get()
        });
        Self((depth == 1).then(Instant::now))
    }
}

impl Drop for Timing {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        if let Some(started) = self.0 {
            add(started.elapsed().as_micros() as u64);
        }
    }
}

/// Counts one run of `us` microseconds.
pub fn add(us: u64) {
    WORK.with(|w| {
        let (total, runs, longest) = w.get();
        w.set((total + us, runs + 1, longest.max(us)));
    });
}

/// Whether a message from another plugin asks for the report.
pub fn is_ask(message: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(message).is_ok_and(|v| v["perfmon"] == 1 && v["ask"] == "report")
}

/// The report: what was measured, the sandbox's memory, and a few counts worth showing. The
/// longest run starts over with each report, so every report covers the time since the last.
pub fn answer(name: &str, version: &str, memory_bytes: u64, gauges: &[(&str, f64)]) -> Vec<u8> {
    let (work_us, calls, max_us) = WORK.with(|w| {
        let (total, runs, longest) = w.get();
        w.set((total, runs, 0));
        (total, runs, longest)
    });
    let gauges: Vec<serde_json::Value> = gauges.iter().map(|(name, value)| serde_json::json!({ "name": name, "value": value })).collect();
    serde_json::json!({ "perfmon": 1, "name": name, "version": version, "memory_bytes": memory_bytes, "work_us": work_us, "calls": calls, "max_us": max_us, "off_tick": true, "gauges": gauges })
        .to_string()
        .into_bytes()
}

/// What Multiversion sends the monitor to be watched without being listed in its config.
pub fn hello(name: &str) -> Vec<u8> {
    serde_json::json!({ "perfmon": 1, "hello": name }).to_string().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(bytes: &[u8]) -> serde_json::Value {
        serde_json::from_slice(bytes).unwrap()
    }

    #[test]
    fn only_the_report_question_is_answered() {
        assert!(is_ask(br#"{"perfmon":1,"ask":"report"}"#));
        assert!(is_ask(br#"{"ask":"report","perfmon":1,"more":true}"#));
        for other in [&b""[..], b"ping", br#"{"perfmon":2,"ask":"report"}"#, br#"{"perfmon":1,"ask":"secrets"}"#, br#"{"perfmon":1}"#, b"[1]"] {
            assert!(!is_ask(other), "{}", String::from_utf8_lossy(other));
        }
        assert_eq!(parsed(&hello("multiversion")), serde_json::json!({ "perfmon": 1, "hello": "multiversion" }));
    }

    #[test]
    fn runs_are_counted_once_and_the_longest_starts_over() {
        WORK.with(|w| w.set((0, 0, 0)));
        add(300);
        add(1200);
        let first = parsed(&answer("multiversion", "1.2.3", 4096, &[("npcs", 3.0), ("displays", 1.0)]));
        assert_eq!((&first["perfmon"], &first["off_tick"]), (&serde_json::json!(1), &serde_json::json!(true)));
        assert_eq!((first["name"].as_str(), first["version"].as_str()), (Some("multiversion"), Some("1.2.3")));
        assert_eq!((first["memory_bytes"].as_u64(), first["work_us"].as_u64(), first["calls"].as_u64(), first["max_us"].as_u64()), (Some(4096), Some(1500), Some(2), Some(1200)));
        assert_eq!(first["gauges"], serde_json::json!([{ "name": "npcs", "value": 3.0 }, { "name": "displays", "value": 1.0 }]));
        add(50);
        let second = parsed(&answer("multiversion", "1.2.3", 4096, &[]));
        // totals keep counting; the longest run is the longest since the first report
        assert_eq!((second["work_us"].as_u64(), second["calls"].as_u64(), second["max_us"].as_u64()), (Some(1550), Some(3), Some(50)));
    }

    #[test]
    fn a_run_inside_a_run_is_not_counted_twice() {
        WORK.with(|w| w.set((0, 0, 0)));
        {
            let _outer = Timing::start();
            {
                let _inner = Timing::start();
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
            assert_eq!(WORK.with(Cell::get).1, 0, "the inner run alone counts for nothing");
        }
        let (total, runs, longest) = WORK.with(Cell::get);
        assert_eq!(runs, 1);
        assert!(total >= 3000 && longest == total, "{total} {longest}");
        assert_eq!(DEPTH.with(Cell::get), 0);
    }
}
