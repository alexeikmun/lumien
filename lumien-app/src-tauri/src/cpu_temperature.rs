use std::{io::{BufRead, BufReader, Write}, path::PathBuf, process::{Command, Stdio}, sync::{Condvar, LazyLock, Mutex}, time::{Duration, Instant}};
use std::os::windows::process::CommandExt;
use std::os::windows::io::AsRawHandle;

const LIBRARY: &str = "LibreHardwareMonitorLib.dll";
const FRESH: Duration = Duration::from_secs(5);

const RETRY: Duration = Duration::from_secs(30);
const MAX_RETRY: Duration = Duration::from_secs(600);

enum Failure {
    Retry,
    Unavailable,
}

#[derive(Default)]
struct Demand {
    last_read: Option<Instant>,
    resumes: u64,
    generation: u64,
}

impl Demand {
    fn renew(&mut self, now: Instant, resumes: u64) -> bool {
        let changed = self.last_read.is_none_or(|last| now.duration_since(last) >= FRESH)
            || self.resumes != resumes;
        self.last_read = Some(now);
        self.resumes = resumes;
        if changed { self.generation = self.generation.wrapping_add(1); }
        changed
    }

    fn remaining(&self) -> Duration {
        self.last_read.map_or(Duration::ZERO, |last| FRESH.saturating_sub(last.elapsed()))
    }
}

struct State {
    demand: Demand,
    reading: Option<(Instant, f64)>,
    status: String,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State {
    demand: Demand::default(),
    reading: None,
    status: "Starting CPU temperature sensor".into(),
}));
static DEMAND_CHANGED: Condvar = Condvar::new();
static START: std::sync::Once = std::sync::Once::new();

fn wait_for_sample(generation: u64, at: Option<Instant>) -> bool {
    let mut state = STATE.lock().unwrap();
    loop {
        let remaining = state.demand.remaining();
        if state.demand.generation != generation || remaining.is_zero() { return false; }
        let delay = at.map_or(remaining, |at| at.saturating_duration_since(Instant::now()));
        if delay.is_zero() { return true; }
        (state, _) = DEMAND_CHANGED.wait_timeout(state, delay.min(remaining)).unwrap();
    }
}

fn run() {
    loop {
        let state = STATE.lock().unwrap();
        let state = DEMAND_CHANGED.wait_while(state, |s| s.demand.remaining().is_zero()).unwrap();
        let generation = state.demand.generation;
        drop(state);

        let mut retry = RETRY;
        while wait_for_sample(generation, Some(Instant::now())) {
            let retry_at = match collect(generation, &mut retry) {
                Ok(()) => break,
                Err(Failure::Unavailable) => None,
                Err(Failure::Retry) => {
                    let at = Instant::now() + retry;
                    retry = (retry * 2).min(MAX_RETRY);
                    Some(at)
                }
            };
            if !wait_for_sample(generation, retry_at) { break; }
        }
    }
}

// CPU-only LibreHardwareMonitor helper; stdout carries Celsius or "unavailable: <reason>", never load estimates.
pub fn read() -> Option<f64> {
    let power = crate::monitor_power::state();
    if !power.on() { return None; }
    START.call_once(|| { std::thread::spawn(run); });
    let mut state = STATE.lock().ok()?;
    if state.demand.renew(Instant::now(), power.resumes) {
        state.reading = None;
        state.status = "Starting CPU temperature sensor".into();
        DEMAND_CHANGED.notify_one();
    }
    state.reading.filter(|(at, _)| at.elapsed() < FRESH).map(|(_, value)| value)
}

/// Why `read()` has no value; `None` while a fresh reading exists.
pub fn status() -> Option<String> {
    let state = STATE.lock().ok()?;
    match state.reading { Some((at, _)) if at.elapsed() < FRESH => None, _ => Some(state.status.clone()) }
}

fn set_unavailable(generation: u64, reason: String) {
    if let Ok(mut state) = STATE.lock() {
        if state.demand.generation != generation { return; }
        if state.status != reason { eprintln!("[cpu-temp] {reason}"); }
        state.reading = None;
        state.status = reason;
    }
}

fn find_library() -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let kanali = |root: Option<PathBuf>| root.map(|r| r.join(r"KANALI\resources\Connect").join(LIBRARY));
    [
        env("LUMIEN_LHM_DLL"),
        std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.join(LIBRARY))),
        kanali(env("ProgramFiles")),
        kanali(env("ProgramFiles(x86)")),
        kanali(env("LOCALAPPDATA").map(|r| r.join("Programs"))),
    ].into_iter().flatten().find(|path| path.is_file())
}

// A permanent failure waits for a new demand session. Other failures back off.
fn collect(generation: u64, retry: &mut Duration) -> Result<(), Failure> {
    let Some(library) = find_library() else {
        set_unavailable(generation, format!("{LIBRARY} not found. Install KANALI, put the DLL next to Lumien's exe, or set LUMIEN_LHM_DLL."));
        return Err(Failure::Retry);
    };
    let Ok(mut child) = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", include_str!("cpu_temperature.ps1")])
        .env("LUMIEN_LHM_DLL", &library)
        .creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        set_unavailable(generation, "Could not start powershell.exe for the CPU temperature sensor".into());
        return Err(Failure::Retry);
    };
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut next_sample = Instant::now();
    let mut line = String::new();
    let result = loop {
        if !wait_for_sample(generation, Some(next_sample)) { break Ok(()); }
        line.clear();
        if writeln!(input, "sample").is_err()
            || !matches!(output.read_line(&mut line), Ok(n) if n > 0) {
            set_unavailable(generation, "CPU temperature helper stopped; will retry while in use".into());
            break Err(Failure::Retry);
        }
        match parse_reading(&line) {
            Ok(value) => {
                *retry = RETRY;
                let mut state = STATE.lock().unwrap();
                if state.demand.generation == generation {
                    state.reading = Some((Instant::now(), value.round()));
                }
            }
            Err(reason) => {
                let permanent = reason.starts_with("CPU temperature needs administrator rights")
                    || reason == "LibreHardwareMonitor found no CPU";
                set_unavailable(generation, reason);
                break Err(if permanent { Failure::Unavailable } else { Failure::Retry });
            }
        }
        next_sample = Instant::now() + Duration::from_secs(1);
    };
    drop(input);
    // Give LibreHardwareMonitor time to close its hardware access on EOF.
    if unsafe { crate::WaitForSingleObject(child.as_raw_handle(), 2000) } != 0 {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

fn parse_reading(line: &str) -> Result<f64, String> {
    let line = line.trim();
    if let Some(reason) = line.strip_prefix("unavailable:") { return Err(reason.trim().to_string()) }
    match line.parse::<f64>() {
        Ok(value) if value.is_finite() && value > 0.0 && value <= 125.0 => Ok(value),
        Ok(_) => Err(format!("CPU sensor returned an implausible value ({line})")),
        Err(_) => Err("CPU temperature unavailable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demand_restarts_only_after_idle_or_resume() {
        let mut demand = Demand::default();
        let now = Instant::now();
        let step = Duration::from_secs(1);
        assert!(demand.renew(now, 0));
        assert!(!demand.renew(now + step, 0));
        assert!(demand.renew(now + step + FRESH, 0));
        assert!(demand.renew(now + step * 2 + FRESH, 1));
        assert_eq!(demand.generation, 3);
    }

    #[test]
    fn accepts_celsius_and_rejects_unavailable_or_invalid_samples() {
        assert_eq!(parse_reading("63.5\r\n"), Ok(63.5));
        for value in ["unavailable", "0", "-1", "NaN", "inf", "300"] {
            assert!(parse_reading(value).is_err(), "{value}");
        }
    }

    #[test]
    fn keeps_helper_reason() {
        assert_eq!(parse_reading("unavailable: needs administrator\r\n"), Err("needs administrator".into()));
    }
}
