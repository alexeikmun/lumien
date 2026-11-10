# Lumien Project Progress & Investigation Log

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
