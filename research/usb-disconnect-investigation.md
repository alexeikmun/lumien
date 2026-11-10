# USB disappearance investigation — 2026-11-10

## Observed evidence
- Transaction log: one 793-byte layout at 12:11:50.963; normal 1 Hz 79–80-byte value updates and ~5–9 ms reads until 12:13:52.917.
- Kernel-PnP/Device Management event 1010 at 12:13:53: exact device USB\\VID_391A&PID_1021\\7DA3448149D8E9DA surprise removed, count 1. DEVPKEY_Device_LastRemovalDate agrees.
- Parent USB\\ROOT_HUB30\\7&3898d510&0&0, port path USBROOT(0)#USB(13). Last arrival 11:57:49; still absent during investigation.
- User reports LCD stays black. Later WriteFailed(5) occurs after removal; it does not establish a permissions problem as original cause.
- AC USB selective suspend was already disabled. No corresponding USB fault in System log; USBXHCI operational log empty. No evidence of over-current, ordinary host sleep or SoC reboot was captured.
- Only lumien-app PID 22508 seen among Lumien/KANALI/Screen/UDB processes at inspection. No repeated Apply in captured interval. Worker race below is real but not demonstrated as trigger for this incident.

## Leading mechanism (not yet confirmed on this unit)
Primary external investigation: https://github.com/DXVSI/Tryx-Linux-GUI/issues/28
Reports include Panorama SE 391a:1021 firmware v2.0.6.20260713 and the same persistent disappearance/black-screen symptom. Their traces distinguish always-pending Bulk IN in KANALI from idle gaps in third-party clients. Keeping IN pending prevented drops for hours; USB link-power-management firmware failure is suspected. Disabling selective suspend is not equivalent to this workaround. This matches Lumien's former synchronous request/read/sleep sequence, but local A/B evidence is still needed.

## Changes
- Added opt-in DisplaySession in lumien-device. Dedicated reader keeps overlapped Bulk IN pending on shared usbprint handle, bounded input queue, frame reassembly, cancellation flag polled every 100 ms without cancelling idle I/O. Startup waits for first pending read before OUT. Zero-byte completions rearm after 20 ms. Terminal errors end session; no automatic layout replay.
- Desktop Apply/Clear use DisplaySession. A single mutex protects command plus ACK and worker replacement; per-worker stop token and join replace shared boolean plus 50 ms sleep. Applied handle moves to telemetry worker; text-only layouts retain idle reader too. ACK parsed before tick success; uncertain transaction stops without retry.
- On unexpected Win32 wait failure, cancel/drain overlapped read/write before returning so stack buffers cannot outlive pending operations.
- Monitor selects exact VID/PID, prefers present nodes over phantoms, handles missing nodes/query errors, emits no invented cause. Separate .pnp.log prevents self-tail duplicate output; handles truncation and partial tail lines.
- Existing direct PanoramaDevice API retained for CLI/brightness/info callers.

## Validation and recovery
Offline: Rust workspace 12 passed; desktop backend 1 passed (worker stop/join); mocked PowerShell monitor test passed. Separate binary build under target/usb-disconnect-fix avoids replacing running executable. No new USB command, reset, power-policy/driver/firmware change or physical recovery performed.
Pending: recover display with controlled full PC shutdown and PSU power removal until standby lights extinguish, then restore power. Save work first; do not disconnect pump/fan wiring. Do not automate shutdown from this session.
Then close old Lumien/KANALI and start patched binary, run process preflight, and deliberately apply one supported layout. Observe at least 30–60 minutes (original failed ~123 s after Apply). Restart monitor to use updated script and correlate transaction + PnP logs. Confirm physical image, reapply/clear sequencing, no detach, and reader shutdown. Only then mark mitigation hardware-validated. If failure persists, capture USB ETW/vendor comparison before changing firmware or cabling.

Final standalone build: `rtk cargo build --offline --manifest-path lumien-app/src-tauri/Cargo.toml --target-dir target/usb-disconnect-fix --features tauri/custom-protocol` passed; embeds existing dist assets (no dev server required). Launch only after old app is closed and display recovered.

### Patched-build stability check completed — 2026-11-10
- User approved closing windowless old process PID 24096 after graceful close failed; explicitly approved force-close following automatic review rejection. Restarted isolated patched executable as PID 9004; no KANALI/Screen/UDB running.
- Applied System Overview again, physical changing-value confirmation recorded in earlier run. Continuous observation started 2026-11-10T12:43:51.1112422-05:00. User shortened planned 30-minute test to ten minutes.
- Read-only observer completed 625.9 seconds (~10 min 26 s), 638 successful telemetry ACKs at recorded checkpoint, zero ERROR/FATAL entries, no PnP removals or stale heartbeat. Exact patched process identity preserved throughout. Final PnP OK. Observation finished 2026-11-10T12:54:22.2608034-05:00.
- Saved report and transaction excerpt under research/evidence/usb-stability-2026-11-10.{json,log}. Observer stopped at requested endpoint; patched app left running.
- Initial ten-minute hardware soak PASSED, exceeding prior ~123-second failure window. This supports idle-IN mitigation, not definitive proof of LPM root cause or multi-hour stability. Repeated Apply/Clear and longer-duration soak remain untested.
