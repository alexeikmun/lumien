//! Read-only host network counters and byte-rate formatting shared by CLI and desktop.
use std::{collections::HashMap, time::{Duration, Instant}};
use windows_sys::Win32::NetworkManagement::IpHelper::{GetIfTable2, FreeMibTable};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetworkRates { pub download: f64, pub upload: f64 }

#[derive(Default)]
pub struct NetworkSampler {
    previous: Option<(Instant, HashMap<u64, (u64, u64)>)>,
    cached: Option<NetworkRates>,
}

impl NetworkSampler {
    pub fn sample(&mut self) -> Result<Option<NetworkRates>, u32> {
        if self.previous.as_ref().is_some_and(|(at, _)| at.elapsed() < Duration::from_millis(750)) {
            return Ok(self.cached);
        }
        let counters = match interface_counters() {
            Ok(counters) => counters,
            Err(error) => { self.previous = None; self.cached = None; return Err(error); }
        };
        Ok(self.update(Instant::now(), counters))
    }

    fn update(&mut self, now: Instant, counters: HashMap<u64, (u64, u64)>) -> Option<NetworkRates> {
        let mut rates = None;
        if let Some((at, previous)) = &self.previous {
            let seconds = now.duration_since(*at).as_secs_f64();
            if seconds > 0.0 {
                let mut total = NetworkRates { download: 0.0, upload: 0.0 };
                for (id, (received, sent)) in &counters {
                    if let Some((old_received, old_sent)) = previous.get(id) {
                        // Reconnects, counter resets and new adapters must not create traffic spikes.
                        total.download += received.saturating_sub(*old_received) as f64 / seconds;
                        total.upload += sent.saturating_sub(*old_sent) as f64 / seconds;
                    }
                }
                rates = Some(total);
            }
        }
        self.previous = Some((now, counters));
        self.cached = rates;
        rates
    }
}

fn interface_counters() -> Result<HashMap<u64, (u64, u64)>, u32> {
    let mut table = std::ptr::null_mut();
    let error = unsafe { GetIfTable2(&mut table) };
    if error != 0 { return Err(error); }
    if table.is_null() { return Err(13); }
    let mut counters = HashMap::new();
    unsafe {
        let rows = std::slice::from_raw_parts(std::ptr::addr_of!((*table).Table).cast(), (*table).NumEntries as usize);
        for row in rows {
            let row: &windows_sys::Win32::NetworkManagement::IpHelper::MIB_IF_ROW2 = row;
            if row.OperStatus == 1 && matches!(row.Type, 6 | 71) && row.InterfaceAndOperStatusFlags._bitfield & 1 != 0 {
                counters.insert(row.InterfaceLuid.Value, (row.InOctets, row.OutOctets));
            }
        }
        FreeMibTable(table.cast());
    }
    Ok(counters)
}

pub fn is_network(binding: &str) -> bool { matches!(binding, "Network download" | "Network upload") }

pub fn format_rate(value: Option<f64>) -> (String, &'static str) {
    let Some(value) = value.filter(|v| v.is_finite() && *v >= 0.0) else { return ("--".into(), "MB/s") };
    let text = format!("{:.2}", value / 1e6);
    (text.trim_end_matches('0').trim_end_matches('.').into(), "MB/s")
}

pub fn format_metric(binding: &str, value: Option<f64>) -> (String, &'static str) {
    if is_network(binding) { return format_rate(value); }
    let text = value.filter(|v| v.is_finite()).map(|v| v.to_string()).unwrap_or_else(|| "--".into());
    (text, if binding.to_lowercase().contains("temp") { "°C" } else { "%" })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deltas_use_elapsed_time_and_survive_reset_and_adapter_changes() {
        let mut sampler = NetworkSampler::default();
        let now = Instant::now();
        assert_eq!(sampler.update(now, HashMap::from([(1, (100, 200)), (2, (500, 900))])), None);
        assert_eq!(sampler.update(now + Duration::from_secs(2), HashMap::from([(1, (2100, 1200)), (2, (1500, 1900))])),
            Some(NetworkRates { download: 1500.0, upload: 1000.0 }));
        assert_eq!(sampler.update(now + Duration::from_secs(3), HashMap::from([(1, (10, 20)), (3, (999_999, 999_999))])),
            Some(NetworkRates { download: 0.0, upload: 0.0 }));
        assert_eq!(sampler.update(now + Duration::from_secs(4), HashMap::new()),
            Some(NetworkRates { download: 0.0, upload: 0.0 }));
    }
    #[test]
    fn byte_rate_units_match_boundaries() {
        for (bytes, text, unit) in [(0.0, "0", "MB/s"), (100_000.0, "0.1", "MB/s"), (1_250_000.0, "1.25", "MB/s"), (2e9, "2000", "MB/s")] {
            assert_eq!(format_rate(Some(bytes)), (text.into(), unit));
        }
        assert_eq!(format_rate(None).0, "--");
        assert_eq!(format_rate(Some(f64::NAN)).0, "--");
    }
}
