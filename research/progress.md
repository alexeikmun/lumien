# Lumien Project Progress & Investigation Log

## Desktop performance review — 2026-11-13

- Editor telemetry and USB discovery now pause while the window is hidden or minimized or Windows reports the monitor off. Hidden sign-in still checks for the LCD while a saved layout is waiting to apply. The visible preview and LCD worker sample only their bound metrics. Unchanged readings skip React state updates, and scene serialization is cached between edits.
- LibreHardwareMonitor starts when a visible or applied CPU temperature widget needs it. Startup warms the helper only for a saved layout that uses it. The helper exits after five seconds without demand, backs off from 30 seconds to ten minutes on retryable errors, and waits for new demand or resume after a privilege or unsupported CPU failure. NVML initialization retries at most once per minute and runs only for GPU widgets. CPU load uses a shared sample at least 900 ms apart.
- Video uploads read from disk in 64 KiB chunks with an incremental CRC32C. Offline tests compare all transfer frames with the previous format and verify a truncated stream never sends the end frame. Download and conversion processes use a Windows process wait with the existing ten-minute timeout. Built-in preset previews and editor fonts are bundled locally. Fonts use the complete upstream files from Google Fonts commit `29b4ec6df2522a8aee279414b78a07910b065874` with their SIL Open Font Licenses. Orbitron ships in its original format to preserve its reserved name. The other fonts use WOFF2.
- USB checks still scan for competing apps immediately before telemetry writes and compare executable names without allocating strings for unrelated processes. Release builds skip USB diagnostic formatting. No Desktop transaction file is created by these changes.
- Deferred playback-first recovery until an LCD test distinguishes a successful play acknowledgement from actual playback of a missing file. The USB reader remains pending because prior disconnect investigation links idle Bulk IN gaps to the black-screen disconnect. Profile the WebView2 tree and USB idle power before considering controller memory mode or closing that reader. Gather zero-length USB read counts only if traces show a recurring wake-up. Gallery thumbnails still use the existing IPC path for downloaded videos and imported presets. Revisit if large libraries show measurable cost. Autostart still repairs and upgrades registration because a sign-in launch can come from either the installer Run key or the elevated task.
- Offline verification passed: 20 workspace tests, 23 desktop tests (one hardware test ignored), 13 frontend tests, TypeScript/Vite production build, and Rust release build. Browser smoke test with simulated Tauri calls covered hidden startup through disconnect/reconnect, bound-only CPU temperature polling, monitor-off pause, wake, and local font and preset preview loading. No physical LCD writes.
- Next: with the current app and KANALI stopped, launch the patched desktop. Confirm saved layout startup from the tray, monitor-off and sleep/resume with live CPU temperature and video, and manual gallery playback. Capture process CPU time and private bytes in the idle tray, live LCD and monitor-off scenarios before considering further changes.
- PC B preflight found one TRYX and no competing writers. The LCD reported enabled at 25%; the saved Main layout has five visible metrics (CPU load, GPU load, GPU temperature, download speed, CPU temperature) and an 83,038,327-byte local video. A direct Cargo release build without the `custom-protocol` feature still opened localhost, so the first hidden-start attempt did not reach the editor. Use `pnpm --dir lumien-app tauri build --no-bundle` or build with `--release --features custom-protocol` after the frontend build for standalone testing.
- PC B patched standalone build started hidden via the existing elevated task. User confirmed moving local video, five visible metrics and a numeric CPU temperature. The monitor stayed on during the measured 607.6 seconds. Windows still enumerated the TRYX afterward. Eight Lumien, WebView2 and PowerShell processes averaged 440.5 MiB private bytes (449.6 MiB peak) and accumulated 4.375 CPU seconds (0.72% of one logical core). The measurement CSV is `%TEMP%\lumien-perf-eb1a78a\patched.csv`.

## Windows monitor power synchronization — 2026-11-13
- Added a CLI display power read/toggle using UserConfig query 104 and response 504, then DisplayConfig field 1 for panel enable and field 2 for unchanged brightness. With other USB writers stopped, the physical TRYX acknowledged and read back on → off → on, at 25% throughout. This verifies panel control through firmware settings, not power removal from the cooler.
- Tauri subscribes to Windows session display and sleep notifications and wakes its existing USB worker when the state changes. Idle monitor-off pauses telemetry and sends panel-off once. Monitor-on restores the brightness recorded before dimming. System resume uses the existing USB reconnect and content restore path. A recovery file keeps the saved brightness available if Lumien exits while the panel is off.
- CLI and desktop builds passed. Protocol tests (15) and desktop worker/power tests (9) passed. Windows registered the callbacks, and an independent probe received the initial display-on notification. The rebuilt desktop applied Main and continued receiving telemetry acknowledgements.
- User confirmed the TRYX turned off with the Windows monitor and restored its content on wake. No matching off/on transactions were present in the debug log checked afterward, so this result is based on physical observation. Full PC sleep, rapid transitions, and the black-image case remain to be checked separately.

### Review follow-up for 8a60aef
- Recheck competing applications before monitor power transactions and live telemetry using a native Windows process snapshot. USB cancellation now uses a Win32 event instead of waking the reader every 100 ms.
- Use the recovery file as an ownership marker and read brightness from the device on wake. A legacy numeric file no longer supplies brightness. If another controller already enabled the panel, clear the marker without writing a power command.
- Preserve worker recovery mode and hand the open USB session back to command handlers. Failed idle commands stay idle; valid connections can be reused. Reconnect restores content on the same connection, with the default preset fallback for content workers. Failed off transitions wait for a new Windows event.
- Handle user-initiated resume when Windows sends no fresh display status, while keeping an explicitly off monitor dark. Display configuration and firmware errors now use the common response parser.
- Offline checks passed: 19 workspace tests, 21 desktop tests (1 ignored), CLI and desktop builds. Read-only CLI query through the revised USB reader reported the TRYX enabled at 25%. Automatic off/on, full sleep, and overlapping controller scenarios still need physical checks for the reviewed build.

## CPU temperature correction — 2026-11-11
- Found fabricated CPU temperature (`40 + CPU load / 4`) in preview telemetry and LCD worker. Replaced both, plus initial layout value, with shared cached CPU-only LibreHardwareMonitor Celsius readings. Uses installed KANALI library through hidden persistent PowerShell helper; prefers CPU Package / Tctl-Tdie, otherwise actual core temperatures. Never uses ACPI thermal zones or CPU load as temperature.
- Invalid/zero/stale (>5 seconds) readings show unavailable. Helper exits on stdin closure and retries failed startup after 30 seconds. Requires installed KANALI LibreHardwareMonitor library and working CPU sensor access; no library bundled or driver installed by this change.
- Focused parser test passed. Local sensor probe returned 0, which is rejected; actual temperature access remains unresolved, possibly privilege/library support. Next: verify sensor access in an elevated Lumien process and compare CPU Package reading with trusted hardware monitor. No LCD writes performed.
- Follow-up: Windows PowerShell 5.1 loaded KANALI's LibreHardwareMonitor 0.9.6 and detected the Ryzen 7 9800X3D's `Core (Tctl/Tdie)`, which returned 0 in the non-elevated session. `sc.exe query PawnIO` confirms the driver is running and its PnP device reports OK. Next: use an administrator PowerShell to run `'sample' | powershell.exe -NoProfile -File .\lumien-app\src-tauri\src\cpu_temperature.ps1`. Compare the returned Celsius value with the same Ryzen sensor in a hardware monitor before testing the LCD.
- Editor now reads live CPU temperature directly and displays `-- °C` when unavailable instead of showing the saved widget value. This also keeps live readings out of validation for saved widget values. Frontend build and all 12 frontend tests passed.
- Installed 0.3.0 reported `-- °C` even after the user tried launching it as administrator. A verified elevated Windows PowerShell probe read Ryzen `Core (Tctl/Tdie)` at 46.87501 °C and CCD1 at 41.375 °C. A Windows-subsystem Rust harness calling the exact production sampler returned 47 °C when elevated. Driver and sampler validated independently of the app. Next: verify the installed app's process elevation and editor/LCD readout. The probe performed no LCD writes.
- User authorized an elevated launch of installed 0.3.0. After checking that KANALI, Screen and UDB were stopped, launched Lumien via RunAs. Editor visibly showed CPU Temp at 48 °C and reported startup apply successful with live 1 Hz updates for all six widgets in Main. Left elevated Lumien running. Physical LCD readout was not observed directly. Normal, non-elevated startup still lacks a verified CPU sensor source.
- Reproduced the user's failing Start menu launch on the same installed executable. Windows TokenElevation confirmed the live process used standard permissions. Created a separate local Start menu shortcut named Lumien (Administrator) targeting the installed executable and requesting UAC. Next: exit the standard tray instance, launch the new shortcut, and verify CPU Temp. No installer or startup changes for this workaround.
- After the user built Tauri as administrator, the active process was the new repository release executable under `lumien-app/src-tauri/target/release`. Windows TokenElevation reported standard permissions. Its CPU sensor helper was running and its HKCU startup entry pointed to that release executable. To reproduce the verified elevated sensor path with this build, exit the tray process and launch the release executable with `Start-Process -Verb RunAs`. No new LCD test in this follow-up.

## 1. Project Identity & Scope
- **Project**: Lumien (Independent, open-source Windows 11 controller for TRYX Panorama SE ARGB 360 LCD display)
- **Target Hardware**: TRYX Panorama SE (USB VID `0x391A`, PID `0x1021`)
- **Current Investigation Phase**: Phase 0 Complete / Phase 1 Complete / Phase 2 & 3 Complete / Phase 4 Complete (Standalone Hardware Brightness Transaction Verified)

---

## 2. Completed Work & Supporting Evidence

### Phase 0: Access Verification & Baseline Establishment
- [x] **Inspected REA capabilities**: REA CLI 6.8.0 / SDK 2.3.1 active via MCP. Decompilation engines (Ghidra, Hopper, IDA) unconfigured on host; established complementary tool workflow ([tool-capabilities.md](tool-capabilities.md)).
- [x] **Discovered Physical Device**:
  - Confirmed USB VID: `0x391A`, PID: `0x1021`, bcdDevice: `0x0310` (v3.10), Speed: HighSpeed (480 Mbps).
  - Serial Number: `7da3448149d8e9da`, Manufacturer: `"RK"`, Product: `"PASE"`.
  - Topology: 1 Configuration, Interface 0 (USB Printer Class 07h, Subclass 01h, Protocol 02h).
  - Endpoints: Bulk OUT `0x01` (512B), Bulk IN `0x81` (512B).
  - Driver: Bound natively to in-box `usbprint.sys` (`usbprint.inf`).
  - Active Device Interface: `\\?\usb#vid_391a&pid_1021#7da3448149d8e9da#{a5dcbf10-6530-11d2-901f-00c04fb951ed}` ([environment.md](environment.md)).
- [x] **Established Baseline State & Hashes**:
  - KANALI v2.6.0 running with active processes `KANALI.exe`, `Screen.exe`, `UDB.exe`.
  - SHA-256 digests computed for all core binaries ([application-architecture.md](application-architecture.md)).

### Phase 1: KANALI Architecture Discovery
- [x] **Deconstructed Process & Module Architecture**:
  - `KANALI.exe`: Electron / Vue 3 / Vite UI frontend.
  - `Screen.exe`: .NET 4.7.2 hardware telemetry collector using `LibreHardwareMonitorLib`.
  - `UDB.exe`: Native 64-bit C++ daemon owning the USB device connection.
  - IPC: Named Pipes `\\.\pipe\TryxBridge` (JSON control protocol), `\\.\pipe\TryxScreenPipe`, and shared memory `Local\TryxMediaPush_`.
  - The UI has zero direct USB access; all hardware communication is delegated to `UDB.exe`.
- [x] **Extracted Display Profile & Media Parameters**:
  - Panorama SE Screen Profile: `panorama2240` (Resolution: **2240 × 1080** pixels).
  - Media Format: MP4 with H.264 stream (`.mp4.h264_2240x1080`).
  - Hardware Rendering: Managed by onboard Rockchip SoC running an applet with LVGL UI overlay.

### Phase 2 & 3: Static Paths & Controlled Runtime Validation (Brightness Transaction)
- [x] **Traced Complete End-to-End Brightness Path**:
  - UI emits JSON `CMD_Set_UserConfig` with `displayConfig.backlightBrightness` (0..100).
  - `UDB.exe` Named Pipe server receives JSON, translates into Protobuf `ReqPackagePb`:
    - `HeaderPb` (tag 1): `version = 1`.
    - `UserConfigPb` (tag 200) -> `DisplayConfigPb` (tag 5) -> `backlight_enable = true` (tag 1), `backlight_brightness = N` (tag 2).
  - Serialized Protobuf payload is prefixed with an 8-byte framing container:
    - Bytes 0..3: ASCII `"TRYX"` (`0x54, 0x52, 0x59, 0x58`).
    - Bytes 4..7: `uint32` little-endian payload length $L$.
  - Dispatched via Win32 `WriteFile` with `OVERLAPPED` on Bulk OUT Endpoint `0x01` (`UDB.exe` RVA `0x61a41`).
  - Device acknowledges over Bulk IN Endpoint `0x81` with a 21-byte `RspPackagePb` frame (`dummy_msg: "ok"`).
- [x] **Disproved Device Exclusivity**:
  - Decompiled `CreateFileW` call in `UDB.exe` (RVA `0x56630`): uses `FILE_SHARE_READ | FILE_SHARE_WRITE` (`0x00000003`).
  - Successfully opened live device interface handle (`0x208`, `err = 0`) concurrently while `UDB.exe` was actively communicating.
- [x] **Authored Protocol Specification**:
  - Created [`protocol-specification.md`](protocol-specification.md) with complete sequence diagram, field tables, hex annotations, and binary RVA citations.
- [x] **Built Offline Rust Wire Codec (`crates/lumien-protocol`)**:
  - Zero-dependency Rust library implementing container framing and brightness Protobuf encoding.
  - Verified with 5 unit tests against captured fixtures for 10%, 28%, 42%, 50%, 100%, and device responses (`cargo test: 5 passed`).

### Phase 4: Standalone Device Validation (Lumien CLI & Live Hardware Test)
- [x] **Implemented Win32 Asynchronous Transport (`crates/lumien-device`)**:
  - Direct SetupAPI enumeration of `GUID_DEVINTERFACE_USB_DEVICE` (`{A5DCBF10-6530-11D2-901F-00C04FB951ED}`).
  - Dynamic discovery of VID `0x391A`, PID `0x1021`.
  - Non-exclusive shared `CreateFileW` with `FILE_FLAG_OVERLAPPED`.
  - Robust bounded completion via `WaitForSingleObject` + `GetOverlappedResult` with `CancelIoEx` timeout cleanup.
  - Buffered frame parser handling chunked Bulk transfers (512-byte packets) and multi-frame boundaries.
- [x] **Verified Disassembly for Protobuf Merge Semantics**:
  - Decompiled `UserConfigPb::MergeFrom` (`UDB.exe` RVA `0x4b350`) and `DisplayConfigPb::MergeFrom` (`UDB.exe` RVA `0x47f90`).
  - Proved that omitted submessages (`work_config`, `poweron_config`, `standby_config`, `filter_config`, `light_config`) and orientation fields are skipped via `test / jz`.
  - Confirmed isolated brightness commands cannot disrupt or blank running video playback.
- [x] **Executed Controlled Standalone Hardware Transactions**:
  - **Read-Only Device Info**: `lumien-cli get-info` transmitted 19-byte query (`get_device_info`) and received 112-byte response in 34.85 ms, returning `product_name: "PASE"`, `serial_number: "BYZLTRYX049869"`, `firmware_version: "v2.0.6.20260713"`, `chip_id: "7da3448149d8e9da"`.
  - **Device Auth Query**: `lumien-cli auth` transmitted 17-byte query and received 21-byte response in 8.06 ms with `device_auth { auth: "ok" }`.
  - **Controlled Brightness Command**: `lumien-cli set-brightness 50` dispatched 21-byte frame (`54 52 59 58 0d 00 00 00 0a 02 08 01 c2 0c 06 2a 04 08 01 10 32`). Win32 Write completed in 398.40 µs; Read completed in 31.93 ms returning 21-byte ACK (`dummy_msg: "ok"`). Backlight physically adjusted to 50% on live hardware.
  - **Reversible Restoration**: `lumien-cli set-brightness 25` dispatched 21-byte frame (`... 10 19`). Write completed in 440.60 µs; Read completed in 31.35 ms with ACK. Backlight physically restored to baseline 25%.
  - Confirmed clean handle closure.

### Phase 5: Display Rendering, Widget, & Media Pipeline Deconstruction
- [x] **Widget Pipeline Deconstruction (`research/widget-pipeline.md`)**:
  - Disassembled KANALI layout engine in `app.asar:out/main/index-1c57b0ec.js` (`ScreenService.handleMessage` and `initTextItemId`).
  - Extracted Protobuf schemas for `lv_gui.proto` (`LabelPb`, `LabelGroupPb`, `RunConfigPb`, `BatchGroupLabelUpdatePb`).
  - Proved hardware GUI is rendered by embedded LVGL engine on the Rockchip RK3568 SoC into a hardware overlay plane above the video plane.
  - Traced 1 Hz telemetry synchronization via lightweight string deltas (`BatchGroupLabelUpdatePb`, tag 300, <40 bytes).
- [x] **Root-Caused Three-Indicator Limit**:
  - Pinpointed exact code in `out/renderer/assets/systemInfo-b52ef617.js` (L1838).
  - Proved the restriction is 100% a client-side Vue checkbox limit to avoid collisions in KANALI's rigid 1-column layout. Neither the Protobuf schema, `UDB.exe`, nor the device firmware limits widget count.
- [x] **Media Pipeline Deconstruction (`research/media-pipeline.md`)**:
  - Proved device firmware has no static bitmap/JPEG mode; all visual content is transcoded to Annex B Raw H.264 elementary streams (`.h264_2240x1080`).
  - Extracted H.264 stream profile: `codec=h264`, `profile=Main`, `level=4.1`, `pix_fmt=yuv420p` (or `yuvj420p`), BT.709, full range, 4-byte start codes (`00 00 00 01`).
  - Verified bitstream equivalence between proprietary `MediaX.dll` and standard FFmpeg CLI (`-c:v libx264 -profile:v main -level 4.1 -pix_fmt yuv420p -f h264`).
  - Traced wire file transfer: `FileTransmitBeginPb` (tag 400) -> `FileTransmitDataPb` (tag 401 in 256 KB chunks) -> `FileTransmitEndPb` (tag 402 with CRC32). Measured transfer speed: 902 KB in 135 ms (~6.68 MB/s).
- [x] **Display Calibration & Physical Panel Mapping (`research/display-calibration.md`)**:
  - Documented physical panel geometry (2240 × 1080 AMOLED, L-shaped 90° curve).
  - Mapped front face ($X \in [0, 1120)$), bend zone ($X \in [1060, 1180]$), and side face ($X \in [1120, 2240]$).
  - Generated pixel-accurate calibration pattern (`scratch/calibration_2240x1080.png` and transcoded `.mp4.h264_2240x1080`).
- [x] **Editor Architecture & Scene Model (`research/editor-architecture.md`)**:
  - Defined transport-independent JSON scene model for Lumien's 2D drag-and-drop editor.
  - Specified clean decoupling between editor session state and hardware scene definition.
  - Established Hybrid Architecture: background Raw H.264 video + device-native LVGL telemetry overlay.

---

## 3. Current Phase & Blockers

- **Current Phase**: **Phase 6: Controlled Widget Hardware Workflow & Application Integration** — [WORKFLOW COMPLETE].
- **Blockers**: None. Full end-to-end hardware workflow validated on live physical display.

---

## 4. Completed Hardware Workflow Milestones & Gate Status

### Gate 1: RunConfigPb Tag Conflict Resolution (201 vs 202) — [RESOLVED AUTHORITATIVELY]
- Extracted binary `FileDescriptorProto` directly from `UDB.exe` at offset `0x1e532a`–`0x1e5d4b`.
- Binary field mapping:
  - `0x1e5650`: `Field #200: user_config -> .Tryx.Config.UserConfigPb`
  - `0x1e5684`: `Field #201: run_config -> .Tryx.Config.RunConfigPb` (Varint prefix: `ca 0c`)
  - `0x1e56b8`: `Field #202: cooler_config -> .Tryx.Cooler.CoolerConfigPb` (Varint prefix: `ca 01`) -> **PUMP AND FAN CONTROL (DANGEROUS / OUT OF SCOPE)**
  - `0x1e5883`: `Field #300: batch_group_label_update -> .Tryx.LVGui.BatchGroupLabelUpdatePb` (Varint prefix: `e2 12`)
- Proven: Tag 201 is `run_config`. Tag 202 is `cooler_config` (pump/fans).

### Gate 2: Supported Widget Fields & Semantics — [RESOLVED]
- Recovered full descriptors for `RunConfigPb`, `LabelGroupPb`, `LabelPb`, `BatchGroupLabelUpdatePb`, `GroupLabelUpdatePb`, `LabelTextUpdatePb`.
- Device firmware runs embedded LVGL; only `Text` and `Metric value` widgets map to native `LabelGroupPb` overlay objects.
- `Gauge`, `Line chart`, `Progress bar`, and `Image` are UI editor preview constructs, not supported by device LVGL overlay.
- Device-verified font: `"roboto-regular"`.

### Gate 3: Bounded Offline Encoders & Geometry Validation — [COMPLETED & TESTED]
- Implemented `encode_run_config_frame`, `encode_clear_run_config_frame`, and `encode_batch_label_update_frame` in `lumien-protocol`.
- Integrated zigzag sint32 codec for `gap_left` and `line_gap`.
- Enforced hard canvas geometry bounds ($X + W \le 2240$, $Y + H \le 1080$), font checks, string lengths ($\le 500$).
- All 11 unit tests pass (`cargo test: 11 passed`).

### Gate 4: CLI Dry-Run & Explicit Apply/Update/Save/Load — [COMPLETED & TESTED]
- Implemented in `lumien-cli`:
  - `lumien-cli apply-widget [--dry-run] [--x <x>] [--y <y>] [--text <str>] [--val <u32>]` (158-byte frame).
  - `lumien-cli update-text <gid> <lid> <text> [--dry-run]` (27-byte frame).
  - `lumien-cli clear-widgets [--dry-run]` (15-byte frame).
  - `lumien-cli save-layout <file>` / `lumien-cli load-layout <file> [--dry-run]`.
- Offline layout roundtrip verified: `save-layout` -> `load-layout --dry-run` validates within 2240×1080 and encodes to 156 bytes.
- Unsupported widgets (e.g. `Gauge`) are strictly rejected on load with exit code 1.

### Gate 5: Pre-Flight & Restoration Plan — [VERIFIED]
- Active process check: `KANALI.exe` = 0, `Screen.exe` = 0, `UDB.exe` = 0.
- Restoration baseline from `%APPDATA%\kanali\store.json`:
  - `waterBlockScreenBrightness: 25`
  - `waterBlockScreenRotate: 270`
  - Active media: `download_43.mp4.h264_2240x1080`
  - Restoration command: `lumien-cli clear-widgets` clears the LVGL overlay plane without disturbing background video loop or rotation.

### Gate 6, 7 & 8: Live Hardware Execution & Physical Verification — [SUCCESSFULLY EXECUTED]
1. **Apply Widget (`RunConfigPb`, Tag 201)**:
   - Command: `lumien-cli apply-widget`
   - Wire Frame: 158 bytes.
   - Bulk OUT Write: completed in 409.30 µs.
   - Bulk IN Read: completed in 5.12 ms with ACK `dummy_msg: "ok"`.
   - **Physical Observation**: User physically verified that `"LUMIEN CPU"` and `"42 °C"` rendered cleanly in cyan (`#67E8D0`) on the physical front panel at $(100, 200)$, with running background video preserved.
2. **Independent Value Update (`BatchGroupLabelUpdatePb`, Tag 300)**:
   - Command: `lumien-cli update-text 100 102 "45"`
   - Wire Frame: 27 bytes.
   - Bulk OUT Write: completed in 404.20 µs.
   - Bulk IN Read: completed in 4.74 ms with ACK `dummy_msg: "ok"`. Total transaction: 7.6 ms.
   - Live hardware updated label 102 text to `"45"` in-place.
3. **Save, Reopen, and Reapply Roundtrip**:
   - Command: `lumien-cli save-layout layouts/verified_hardware_layout.json --x 100 --y 200 --text "LUMIEN CPU" --val 45`
   - Successfully written to disk as Scene v2 JSON.
   - Command: `lumien-cli load-layout layouts/verified_hardware_layout.json`
   - Parsed, validated, encoded 158 bytes, written in 451.40 µs, acknowledged in 13.14 ms with `ok`.
4. **Display Restoration**:
   - Command ready: `lumien-cli clear-widgets` sends 15-byte empty `RunConfigPb` to restore clean display.

### Gate 9: Typed Real Adapter & Scene Validation in UI — [COMPLETED & TESTED]
- Implemented `validateHardwareScene` and `RealDeviceAdapter` in `lumien-app/src/adapter.ts`.
- Unsupported widgets (`Gauge`, `Line chart`, `Progress bar`, `Image`) are kept preview-only and strictly rejected before hardware dispatch.
- Implemented narrow Tauri commands `apply_scene` and `update_metrics` in `lumien-app/src-tauri/src/main.rs`.
- Passes all TypeScript/Vite builds (`tsc --noEmit && vite build`) and Node tests (`4/4 passed`).

---

## 5. Precise Hardware Verification Status

| Operation | Physical Device Verification Status | Wire / Byte / Unit Test Status |
| :--- | :--- | :--- |
| **Device Info (`get_device_info`, 100)** | **Physically Verified** (34.85 ms, returned SN BYZLTRYX049869) | Verified against captured fixtures |
| **Device Auth (`get_device_auth`, 101)** | **Physically Verified** (8.06 ms, returned `auth: "ok"`) | Verified against captured fixtures |
| **Brightness Control (`set_brightness`, 200)** | **Physically Verified** (Backlight 50% & 25% confirmed live) | Verified against captured fixtures (10..100) |
| **Widget Layout (`RunConfigPb`, 201)** | **Physically Verified** (Visual render confirmed by user at $(100, 200)$) | 158B frame, 409 µs write, 5.12 ms ACK |
| **Runtime Value Delta (`BatchUpdate`, 300)** | **Physically Verified** (27B frame, 404 µs write, 4.74 ms ACK) | Wire match against verified fixture |
| **Layout Save / Reopen Roundtrip** | **Physically Verified** (`layouts/verified_hardware_layout.json` reapplied) | Validated against Scene v2 schema |
| **Clear Display Overlay (`clear-widgets`)** | **Verified Ready** (15B frame `ca 0c 00`) | Reversible restoration command |
| **Unsupported Kind Rejection** | N/A (Software Gate) | Verified in CLI & `adapter.ts` (Gauge/Chart blocked) |

| :--- | :--- | :--- |
| **Device Info (`get_device_info`, 100)** | **Physically Verified** (34.85 ms, returned SN & FW) | Verified against captured fixtures |
| **Device Auth (`get_device_auth`, 101)** | **Physically Verified** (8.06 ms, returned `auth: "ok"`) | Verified against captured fixtures |
| **Brightness Control (`set_brightness`, 200)** | **Physically Verified** (Backlight 50% & 25% confirmed) | Verified against captured fixtures (10..100) |
| **Widget Layout (`RunConfigPb`, 201)** | **Pending Hardware Reconnect** (Code 45 CM_PROB_PHANTOM) | Verified via binary descriptor & 158-byte dry-run |
| **Runtime Text Update (`BatchUpdate`, 300)** | **Pending Hardware Reconnect** (Code 45 CM_PROB_PHANTOM) | Verified via binary descriptor & 27-byte dry-run |
| **Clear Widgets (`RunConfigPb` empty)** | **Pending Hardware Reconnect** (Code 45 CM_PROB_PHANTOM) | Verified via 15-byte dry-run |
| **Layout Save / Reopen Roundtrip** | **Verified Offline** (`layouts/sample_layout.json`) | Validated against Scene v2 schema |

- Launch: from lumien-app run pnpm tauri dev. Browser-only: pnpm dev. If Vite already runs on 1420, use pnpm tauri dev --config reuse-server.json. Preview server and native app were left running.
- Next: verify native image selection/export/reopen and keyboard workflow, improve preview chart data options if needed; recover authoritative layout tag and CLI hardware evidence independently. Real adapter remains absent until that work completes. No hardware writes performed.

## 2026-11-10 USB disconnect investigation
See [usb-disconnect-investigation.md](usb-disconnect-investigation.md).
- Windows event 1010 confirms one-device surprise removal at 12:13:53; still absent, LCD remains black per user. AC selective suspend already disabled. No over-current/reboot evidence.
- Matching primary external issue identifies idle Bulk IN gaps as likely firmware/link-power trigger. Added continuous input DisplaySession mitigation, serialized/joined desktop workers, telemetry ACK validation, overlapped wait-failure cleanup, and corrected monitor diagnostics/duplicate output.
- Offline checks: workspace 12 tests, desktop 1 test, PowerShell mocked monitor check pass; isolated desktop binary built. Existing running app not restarted and no hardware writes/power/driver settings changed.
- Exact next: save work, manually cold power-cycle PC/display to recover absent USB device; close old Lumien and vendor processes, launch target/usb-disconnect-fix/debug/lumien-app.exe, start updated monitor and apply one supported layout. Observe 30–60+ minutes, test Apply/Clear sequencing, record physical and PnP results. Candidate workaround not yet hardware-validated.

### Recovered-device validation — 2026-11-10 12:34–12:37 Bogota
- User reported LCD recovered. PnP OK/Present/CM_PROB_NONE; check-prereqs found no KANALI/Screen/UDB. Vendor brightness recorded 25%; no brightness changes.
- Launched isolated patched standalone binary, applied System Overview at 12:34:05.337; ACK at 12:34:05.424. User confirmed physical dashboard visible and values changing.
- 70 parsed successful telemetry ACKs through 12:35:15.953, no logged errors. Patched process then disappeared; different executable under lumien-app/src-tauri/target/debug was found at 12:37:33. Source main.rs and that executable were modified during observation by external work; preserved. Test interrupted, cannot claim survival past original ~123 s failure point. USB still present at final PnP check.
- Asked user whether to restart isolated patched build or keep current application. Extended stability validation pending; no repeated writes while other executable active.

### Patched-build stability check completed — 2026-11-10
- User approved closing windowless old process PID 24096 after graceful close failed; explicitly approved force-close following automatic review rejection. Restarted isolated patched executable as PID 9004; no KANALI/Screen/UDB running.
- Applied System Overview again, physical changing-value confirmation recorded in earlier run. Continuous observation started 2026-11-10T12:43:51.1112422-05:00. User shortened planned 30-minute test to ten minutes.
- Read-only observer completed 625.9 seconds (~10 min 26 s), 638 successful telemetry ACKs at recorded checkpoint, zero ERROR/FATAL entries, no PnP removals or stale heartbeat. Exact patched process identity preserved throughout. Final PnP OK. Observation finished 2026-11-10T12:54:22.2608034-05:00.
- Saved report and transaction excerpt under research/evidence/usb-stability-2026-11-10.{json,log}. Observer stopped at requested endpoint; patched app left running.
- Initial ten-minute hardware soak PASSED, exceeding prior ~123-second failure window. This supports idle-IN mitigation, not definitive proof of LPM root cause or multi-hour stability. Repeated Apply/Clear and longer-duration soak remain untested.

## Editor document workflow and startup — 2026-11-10
- Toolbar: native Open/Save/Save As via Tauri dialog backend; disk-confirmed atomic overwrite, saved-path tracking, Save/Discard/Cancel, clean per-document history and grouped rename/drag undo. Live readings separated from persisted scene. Native UI verified rename/undo/redo, chosen-file save/overwrite/reopen, new, cancel, save-before-new, invalid-file rejection, Save As cancel.
- Recent-layout path persisted in app config recent-layout.json only after accepted Open or successful Save. Startup re-reads file, validates it and restores clean history/saved state. Missing/invalid paths show a visible error. Native close/relaunch verified current Main layout (3 widgets) automatically restored. User Escape stopped optional Save As restart check. Original prior exported six-widget file was later edited by external/user work to Main/3; preserved current contents.
- Startup auto-apply now implemented at user request: waits for valid restored scene + USB availability, consumes pending scene before writing, no automatic retry/reconnect replay, cancels pending write after edits/New/Open or explicit Apply/Clear. Native write preflight rejects KANALI/Screen/UDB/other Lumien writers; errors visible. Default sample is never automatically sent when restore fails or no recent file exists.
- Offline checks at implementation: TypeScript/Vite production build; frontend 8 tests; desktop 4 tests. Final startup auto-apply hardware verification pending. Existing transport framing/brightness paths preserved.

### Automatic startup Apply verified — 2026-11-10 13:25 Bogota
- Current saved Main layout contains 3 metric widgets. Restarted new standalone target/startup-apply/debug/lumien-app.exe (PID 25288) after gracefully closing saved prior build. Preflight: no KANALI/Screen/UDB/other Lumien writer, PnP OK, vendor brightness baseline 25 recorded and unchanged.
- Without clicking Apply, startup restored Main and transmitted exactly one RunConfig at 13:25:03.155 (427 bytes); parsed OK ACK at 13:25:03.174, followed by successful live telemetry ACKs. Evidence: research/evidence/startup-apply-2026-11-10.{json,log}. This confirms automatic device transaction; physical panel observation is not inferred from ACK.
- Tests/build: frontend 8 tests, desktop 4 tests, production TypeScript/Vite and standalone Rust builds passed. Startup gate tests cover delayed USB, once-only dispatch, edit cancellation and absent restored scene. No new dependencies. Native preflight blocks competing writers, including another Lumien process.
- New build left running. Missing/invalid remembered file never sends sample scene; no automatic retry on uncertain write or reconnection after attempt. A changed document cancels pending startup Apply. Longer stability soak not repeated for this frontend feature.

## Network speed sensors — 2026-11-10
- Added Network download / Network upload bindings to shared seven-sensor catalog, palette and inspector. Values are bytes/s; preview and LCD format B/s, KB/s, MB/s, GB/s with matching thresholds. Unavailable/warm-up readings show --. Scene v2 unchanged; network fallbacks allow finite nonnegative rates above 100, other metrics retain prior validation.
- New lumien-telemetry crate shared by CLI + desktop uses Windows GetIfTable2 64-bit InOctets/OutOctets via already-installed windows-sys bindings; sums operational physical Ethernet/Wi-Fi only, excludes virtual/loopback. Rate deltas use monotonic elapsed time, adapter identities, reset/hotplug handling, shared sampling cache. No admin or USB writes for host sampling.
- Read-only CLI `lumien-cli network` sampled real traffic (357 B/s download, 415 B/s upload in initial check). Existing layout/tag-300 commands reused; initial and live network labels update numeric value and unit together. CLI multi-widget group IDs fixed at shared builder to avoid collisions.
- Prepared layouts/main-with-network.lumien.json as separate copy of current Main plus two network widgets in unused left face; original Main file untouched. CLI dry-run successfully encoded 5 metrics, including fractional/high-rate fallback test.
- Checks: frontend 9 tests; Rust workspace 14 tests; desktop 5 tests. Sampling/reset/hotplug math, unit boundaries, nullable readings, above-100 persistence, and LCD unit/value label IDs covered. Native UI and hardware observations recorded below.

- Native UI verified in running rebuilt development app: palette shows seven sensors; opened saved Main + Network (five metrics), observed changing download/upload preview and B/s → KB/s → MB/s units. Applied through UI at 13:56:21; device returned OK RunConfig ACK (715-byte frame) and consecutive live update ACKs with no errors in captured interval. Evidence: research/evidence/network-sensors-2026-11-10.{json,txt}. Recent-layout pointer now selects separate Main + Network file; original Main preserved. User subsequently confirmed both DOWNLOAD and UPLOAD visible and changing on physical LCD; no new ten-minute soak claimed.
- Final checks rerun: 14 workspace + 5 desktop + 9 frontend tests passed, TypeScript/Vite production build and standalone target/network-sensors desktop build passed, git diff --check clean. Next: network sensor milestone complete with user-confirmed physical rendering. If investigating rate accuracy, compare read-only lumien-cli network against Windows adapter counters; extended network-layout stability/throughput accuracy test has not been repeated.

### Network units preference update — 2026-11-10
- User requested MB/s display instead of KB/s: preview, palette, inspector, CLI and LCD now use fixed MB/s with up to two decimal places (100 KB/s → 0.1 MB/s). Raw saved rates remain bytes/s. Below 0.005 MB/s rounds to 0; unavailable remains --. Physical rendering of revised units not yet observed.
- Verification: workspace 14, desktop 5, frontend 9 tests passed; production frontend build passed. No layout files changed.

## Font Styles & Bold Rendering Resolution — 2026-11-10
- Reverse-engineered `device_daemon.elf` font resolution at `0x4CE74` and indexing at `0x4c660`.
- Verified SoC filesystem only contains `-Regular.ttf` for Orbitron, BitcountSingle, and RubikGlitch. Requesting `Orbitron-Bold` triggers daemon label deletion (`DeleteSubrange`) with `partial_ok`, dropping labels from physical LCD.
- Resolved in `lumien-cli` and `lumien-app/src-tauri`: `resolve_device_font()` maps non-Roboto families to `-Regular`, preventing label deletion. Added `roboto-black` (weight 87/900) for heavy bold display on AMOLED panel.
- Frontend Inspector disables Bold/Italic for non-Roboto families with explanatory hint; preview reflects accurate regular weight and 900 weight for Roboto Black.
- Verified on physical hardware via `lumien-cli apply-widget --font "roboto-bold"` and `--font "roboto-black"`; both acknowledged with `ok` in ~10ms. All 14 Rust tests and 10 Node tests pass. Production frontend and desktop binaries built.

## System Tray Icon & Background Execution — 2026-11-10
- Configured Tauri 2 `tray-icon` feature in `lumien-app/src-tauri` using in-box `icons/icon.ico` resource asset.
- Wired `on_window_event` close interception (`api.prevent_close()` + `window.hide()`) so closing or hiding the window keeps the application and background 1 Hz telemetry loop running undisturbed.
- Configured tray left-click / double-click event handler: calls `show_main_window` (`unminimize`, `show`, `set_focus`) strictly on existing `"main"` window instance, preventing duplicate window creation.
- Configured tray context menu with `Settings` (shows/focuses main window) and `Close` (removes tray icon, safely drops telemetry sync worker, and exits cleanly via `app.exit(0)`).
- Verified via automated headless Win32 tests: sending `WM_CLOSE` to window hides window (`Visible: False`) while process remains alive in background (`Process alive? True`). All 14 workspace + 8 desktop + 10 frontend tests passing.

## NSIS Installer Packaging (`makensis`) — 2026-11-10
- Authored [`installer.nsi`](file:///c:/Users/Alexis/code/lumien/installer.nsi) following pattern from `code/clipped`:
  - Solid LZMA compression.
  - Per-user execution level (`$LOCALAPPDATA\Programs\Lumien`).
  - Packages release `lumien-app.exe`, `lumien-cli.exe`, `README.md`, and `icon.ico`.
  - Creates Start Menu and Desktop shortcuts with icon asset.
  - Generates full Windows Add/Remove Programs registry entries and uninstaller (`uninstall.exe`).
- Added [`scripts/build-installer.ps1`](file:///c:/Users/Alexis/code/lumien/scripts/build-installer.ps1):
  - Preflights `makensis.exe` presence in PATH or Program Files.
  - Builds Vite frontend, `lumien-app` release binary, and `lumien-cli` release binary.
  - Runs NSIS compilation and generates standalone release zip + `SHA256SUMS.txt` in `releases/download/`.
- Verified compilation with `makensis.exe`: produces `releases/download/lumien_0.1.0_x64-setup.exe` (2.0 MB). Installed package metadata matches product specs.

## Toolbar Options Persistence (`grid`, `snap`, `split`, `bend`) — 2026-11-10
- Added `grid`, `snap`, `split`, `bend` booleans directly into `Scene` definition in [`lumien-app/src/scene.ts`](file:///c:/Users/Alexis/code/lumien/lumien-app/src/scene.ts).
- Default values: `grid: true`, `snap: false`, `split: true` (division line at 1120px), `bend: true` (bend overlay at 1060–1180px).
- Fully backward compatible: `parseScene()` defaults missing fields gracefully from legacy `.lumien.json` files.
- Connected toolbar checkboxes in [`lumien-app/src/main.tsx`](file:///c:/Users/Alexis/code/lumien/lumien-app/src/main.tsx) to document state: toggling commits changes to undo/redo history, flags document as dirty, and saves state to layout JSON on Save.
- Verified: 11 node tests pass (including roundtrip and legacy parsing), 14 cargo workspace tests pass. Release and dev desktop binaries updated.

## Solid background investigation — 2026-11-11
- Existing uncommitted background implementation preserved. Logs show successful file transfers and WorkConfig acknowledgements even when user reported no visible color; transport acknowledgement alone did not prove playback.
- FFprobe identified embedded swatches as High profile with B frames, differing from documented Main-profile static-image pipeline. Regenerated eight palette assets as Annex B Main@4.1, 2240x1080, yuv420p, 30 intra frames/no B frames. Custom generation uses same settings and separate solid-media-v2 cache to avoid stale encodings. This compatibility change is a candidate fix pending physical observation, not yet a proven root cause.
- Removed silent black fallback on custom encoding failure/missing FFmpeg; errors now surface instead of reporting selected color applied. Validate six-digit hex before constructing cache paths.
- No Lumien/KANALI/Screen/UDB processes active before CLI test; USB present. Uploaded regenerated #18262e via existing CLI set-solid-color: 16587 bytes, transfer 67.43ms, activation acknowledged. Brightness untouched. User asked whether physical background changed; response pending.
- Checks: workspace baseline 17 tests passed; desktop 17 tests passed after patch; frontend 12 tests and production build passed. Existing unrelated style.css EOF blank-line diff warning left unchanged. Next: collect physical observation; if unchanged inspect playback filename/directory semantics and device decoder evidence before claiming resolved.
- Physical follow-up: user described #18262e test as black. Uploaded white regenerated asset (16466 bytes, transfer 61.27ms, activation ACK); user confirmed "White background visible". Confirms regenerated solid-media playback works on actual LCD. Does not isolate High profile/B frames as sole original cause; dark shades need perceptual/color calibration. Desktop custom-protocol debug build succeeded.
- Started rebuilt lumien-app/src-tauri/target/debug/lumien-app.exe after successful build; white hardware test background left active (saved scene has no explicit background preference). Existing layout preserved. Next: verify palette/custom colors in rebuilt app; exact reproduction with original High-profile asset was not performed, so codec change remains compatibility fix rather than proven sole cause.

## Background color picker palette — 2026-11-11
- Made native full-spectrum background color picker prominent with Choose any color label, larger color well, accessible input labels and focus outlines. Expanded existing palette from eight dark/neutral choices to sixteen including vivid colors; selected swatches expose pressed state. Existing preview, hex entry, save and Apply to Display handlers reused.
- Custom/vivid colors use existing FFmpeg generation path; built-in eight swatches remain bundled. No hardware writes performed for this UI change.
- Verification: frontend 12 tests and TypeScript/Vite production build passed. Desktop palette build uses target/network-sensors to preserve running app and unsaved work; native visual check not performed.

## YouTube URL full-screen playback — 2026-11-11
- Added Background options YouTube link / Play full screen with native async download, conversion and existing serialized USB upload/playback. Shared CLI prepare-youtube and play-video exercise same implementation. Strict YouTube ID parsing; public recorded videos <=5 minutes and <=128 MiB; silent, center-cropped playback. Dependencies and operational limits: research/youtube-playback.md.
- User supplied krDWc30PAGg; download/transcode succeeded. GOP-30 + clear-after-activation initially black/frozen; intra-only + clear-before-activation produced user-confirmed moving physical video. Final encoder/order use working combination; causal isolation not claimed. Existing saved layout preserved; runtime video hides overlays.
- Offline tests: workspace 18 and desktop 18 pass; TypeScript/Vite build passes. Built target/network-sensors/debug/lumien-app.exe and launched for UI verification. Official yt-dlp installed to app config tools, uses existing FFmpeg and Node. No pump/fan/backlight changes.

## Downloaded video gallery — 2026-11-11
- Added persistent disk-backed gallery under Backgrounds: local thumbnails, titles, duration, file size and one-click replay without YouTube download/conversion. Existing app-config youtube/job downloads are discovered automatically; new downloads refresh gallery after completion. CLI research files in target/youtube are not application-library items.
- Replay resolves an opaque job/file ID within canonical app library, rejects traversal/outside-library paths, then reuses validated youtube::play under USB worker lock. New transcodes write .partial then rename atomically so gallery excludes in-progress videos. Thumbnails generated locally via existing FFmpeg and cached; missing thumbnails have a placeholder.
- Validation: workspace 18 tests, desktop 19 tests (includes discovery, old-file compatibility, partial exclusion and invalid selection), frontend 12 tests, TypeScript/Vite build and standalone desktop build passed. Native gallery/replay verification pending.
- Native verification complete: rebuilt app discovered all 8 existing app-library downloads with titles, durations, sizes and local first-frame thumbnails. Selected Video Background Smoke HD; UI showed Selected / Play again and Saved video playing full screen. Hardware log at 2026-11-11 14:00:23.348 confirms Gallery playback acknowledged for youtube_GUK6zoLzZXQ.mp4.h264_2240x1080. No download or conversion during replay; saved scene remained Saved. Physical panel observation not repeated; first-frame thumbnails can be black for videos with black intros. New build left running.

## Gallery duplicate and thumbnail polish — 2026-11-11
- Gallery now deduplicates using YouTube video ID, keeping newest completed copy. For 8 local files with repeated IDs, gallery shows 5 unique videos.
- Thumbnail extraction samples a 100-frame window instead of first frame, avoiding black intro cards; new .thumb.jpg cache regenerates old images.
- Frontend production build and alternate target native build passed. Existing Lumien process held default executable lock, so rebuilt app at target/network-sensors/debug/lumien-app.exe; live gallery visual verification waits until that build is launched/reloaded.

## Delete downloaded videos — 2026-11-11
- Added trash-can action per gallery item with confirmation. Backend removes all local job folders for selected video ID, including duplicate transcodes, metadata and thumbnails; canonical app-library paths only. Gallery removes card immediately after success. Reversible confirmation required before deleting; saved device playback is untouched.
- Frontend build and desktop native build passed; git diff --check clean. No hardware writes. Native UI refresh pending.

## Layout preview picker — 2026-11-11
- Replaced Open button/file chooser with layout dropdown. Saved layouts are catalogued by path; last selected/saved layout is included and invalid or missing entries are hidden. Dropdown selection loads scene into editor preview only; device changes only after explicit Apply to Display. Existing save, Save As, dirty-document guard and startup restore/apply behavior remain.
- Checks: `rtk pnpm build` passed; offline Tauri native build passed using `target/network-sensors`. No hardware writes and no existing tests run. Existing layouts saved outside Lumien's catalog and not previously selected are not auto-discovered; Save As adds them. Next: visually verify dropdown options and confirm LCD stays unchanged until explicit Apply.

## Saved layout backgrounds — 2026-11-11
- Root cause: Scene v2 already serialized solid `bgType`/`bgColor`, but selecting a layout did not copy its color into the picker/preview state. Animated preset selection only saved `bgType: preset`; it discarded the preset filename. Startup and Apply restored solid backgrounds only, never the selected preset animation.
- Added optional validated `bgFile` to scene serialization. Preset selection now records exact device media filename; solid selection clears it. Layout load synchronizes color controls and thumbnail preview. Apply and startup restore solid video or the saved preset filename before sending widget overlays. Legacy layouts without background metadata inherit current active device background where possible; solid-color media filename is recognized as a color. Invalid media paths are rejected; unknown thumbnails still allow Apply by stored filename.
- Checks: `rtk pnpm build`, offline Tauri native build (`target/network-sensors`), and `rtk git diff --check` passed. No tests run and no hardware writes. Hardware readback/visual check remains unverified; next launch the new app, save then reload one solid-color layout and one Kanali animation layout, and confirm previews/backgrounds restore independently.

## Preserve widgets during background changes — 2026-11-12
- Video playback explicitly cleared the overlay and background commands stopped the live sensor worker. All desktop background commands now use the same USB transaction to switch media, resend the last active layout, and resume its sensor updates. A transfer or overlay restore failure keeps the previous layout available for automatic recovery. Solid and preset buttons no longer apply whatever layout happens to be open in the editor.
- Frontend, offline desktop, and offline CLI builds passed. No hardware writes. Next: apply a layout with live metrics, then switch among a solid color, a KANALI preset, a YouTube link, and a downloaded video. Confirm both background playback and changing sensor values on the LCD after each switch, especially video activation with the overlay present.

## Restore the last background on relaunch — 2026-11-12
- Startup previously let a saved layout's background replace the last successfully activated background. It now restores the remembered display background first, then reapplies the saved widgets. With no saved layout, startup restores the background alone.
- Playing a YouTube or downloaded video now sets a background filename on the editor scene so it can also be saved in the layout. Startup, Apply and wake recovery look up downloaded videos in the local library and re-upload the prepared stream. If a remembered download is gone, startup falls back to the layout background. A failed background restore is reported and startup still applies the widgets.
- Frontend and offline desktop builds passed. No hardware writes. Next: play a cached video over a saved layout with live metrics, fully exit from the tray, relaunch the rebuilt app and confirm video and metrics return without resaving. Then verify wake recovery and fallback when the cached video is removed.

## Import MP4 backgrounds — 2026-11-12
- Backgrounds → Video → Import MP4 opens a native file picker and converts the first five minutes of the selected file into a silent LCD stream using the shared FFmpeg encoder. The import stays local and appears in the video library under the original filename stem. Removing the library entry removes Lumien's converted copy.
- Imported clips use the existing video playback, widget restore, layout save, and app relaunch paths. Frontend, offline desktop, and offline CLI builds passed. Next: import a local MP4, select it over a live layout, and verify LCD playback and restoration after relaunch.

## Select and edit multiple widgets — 2026-11-12
- Ctrl+click toggles widgets in the canvas or Layers panel. Dragging and arrow keys move the unlocked selection as a group, keeping spacing and clamping the group to the display edges. Snap aligns the dragged widget to the grid and moves the group by the same offset. The inspector offers bulk duplicate, visibility, lock, and delete actions. Locked widgets are skipped by movement and deletion. Each group edit is one undo step.
- Frontend build and 13 tests passed, including group bounds, spacing, locks, and undo. Next: try Ctrl+click, drag with Snap, and Delete in the desktop editor. Apply the edited layout when ready to send it to the LCD.
