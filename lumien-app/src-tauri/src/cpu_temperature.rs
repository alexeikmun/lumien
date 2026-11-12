use std::{io::{BufRead, BufReader, Write}, path::PathBuf, process::{Command, Stdio}, sync::{LazyLock, Mutex}, time::{Duration, Instant}};
use std::os::windows::process::CommandExt;

const LIBRARY: &str = "LibreHardwareMonitorLib.dll";
const FRESH: Duration = Duration::from_secs(5);

struct State { reading: Option<(Instant, f64)>, status: String }

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State { reading: None, status: "Starting CPU temperature sensor".into() }));
static START: std::sync::Once = std::sync::Once::new();

fn start() {
    START.call_once(|| { std::thread::spawn(|| loop {
        collect();
        std::thread::sleep(Duration::from_secs(30));
    }); });
}

// CPU-only LibreHardwareMonitor helper; stdout carries Celsius or "unavailable: <reason>", never load estimates.
pub fn read() -> Option<f64> {
    start();
    STATE.lock().ok()?.reading.filter(|(at, _)| at.elapsed() < FRESH).map(|(_, value)| value)
}

/// Why `read()` has no value; `None` while a fresh reading exists.
pub fn status() -> Option<String> {
    start();
    let state = STATE.lock().ok()?;
    match state.reading { Some((at, _)) if at.elapsed() < FRESH => None, _ => Some(state.status.clone()) }
}

fn set_unavailable(reason: String) {
    if let Ok(mut state) = STATE.lock() {
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

fn collect() {
    let Some(library) = find_library() else {
        return set_unavailable(format!("{LIBRARY} not found. Install KANALI, put the DLL next to Lumien's exe, or set LUMIEN_LHM_DLL."));
    };
    let Ok(mut child) = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", include_str!("cpu_temperature.ps1")])
        .env("LUMIEN_LHM_DLL", &library)
        .creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return set_unavailable("Could not start powershell.exe for the CPU temperature sensor".into());
    };
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut reported = false;
    loop {
        if writeln!(input, "sample").is_err() { break }
        let mut line = String::new();
        if !matches!(output.read_line(&mut line), Ok(n) if n > 0) { break }
        match parse_reading(&line) {
            Ok(value) => if let Ok(mut state) = STATE.lock() { state.reading = Some((Instant::now(), value.round())) },
            Err(reason) => { reported = true; set_unavailable(reason) }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let _ = child.kill();
    let _ = child.wait();
    if !reported { set_unavailable("CPU temperature helper stopped; retrying in 30 seconds".into()) }
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
    use super::parse_reading;

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
