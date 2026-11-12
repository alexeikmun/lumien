use std::{io::{BufRead, BufReader, Write}, process::{Command, Stdio}, sync::{LazyLock, Mutex}, time::{Duration, Instant}};
use std::os::windows::process::CommandExt;

static READING: LazyLock<Mutex<Option<(Instant, f64)>>> = LazyLock::new(|| Mutex::new(None));
static START: std::sync::Once = std::sync::Once::new();

// CPU-only LibreHardwareMonitor helper; stdout carries Celsius, never load estimates.
pub fn read() -> Option<f64> {
    START.call_once(|| { std::thread::spawn(|| {
        loop {
            collect();
            std::thread::sleep(Duration::from_secs(30));
        }
    }); });
    READING.lock().ok()?.filter(|(at, _)| at.elapsed() < Duration::from_secs(5)).map(|(_, value)| value)
}

fn collect() {
    let Ok(mut child) = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", include_str!("cpu_temperature.ps1")])
        .creation_flags(0x08000000).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return };
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    loop {
        if writeln!(input, "sample").is_err() { break }
        let mut line = String::new();
        if !matches!(output.read_line(&mut line), Ok(n) if n > 0) { break }
        let value = parse_reading(&line).map(f64::round);
        if let Ok(mut reading) = READING.lock() { *reading = value.map(|v| (Instant::now(), v)); }
        std::thread::sleep(Duration::from_secs(1));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn parse_reading(line: &str) -> Option<f64> {
    line.trim().parse::<f64>().ok().filter(|value| value.is_finite() && *value > 0.0 && *value <= 125.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepts_celsius_and_rejects_unavailable_or_invalid_samples() {
        assert_eq!(super::parse_reading("63.5\r\n"), Some(63.5));
        for value in ["unavailable", "0", "-1", "NaN", "inf", "300"] {
            assert_eq!(super::parse_reading(value), None);
        }
    }
}
