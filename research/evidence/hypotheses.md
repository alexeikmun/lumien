# Hypotheses Under Investigation

## HYP-001: Device Onboard Operating System & Architecture
- **Hypothesis**: The TRYX Panorama SE LCD display is powered by an embedded Rockchip SoC (likely RV1106, RV1109, RK3308, or RK3566) running an embedded Linux userland with an LVGL / DirectFB display runner applet.
- **Supporting Leads**:
  - USB string descriptor reports manufacturer `"RK"`.
  - Firmware update batch scripts (`panork_flash_firmware.bat`) invoke `rkdeveloptool.exe` with partition layout (`MiniLoaderAll.bin`, `parameter.txt`, `uboot`, `trust`, `boot`, `recovery`, `rootfs`, `oem`, `userdata`).
  - Protobuf definitions include `Tryx.LVGui` (`BatchGroupLabelUpdatePb`, `LabelGroupPb`, `LabelTextUpdatePb`), referencing LVGL (Light and Versatile Graphics Library).
  - PnP and device IDs mention `PanoRK`.
- **Validation Required**:
  - Passive inspection of firmware image structure and binaries in `%APPDATA%\kanali\panoRKFirmware` or installation packages.

## HYP-002: Framing and Header Wrapper over USB
- **Hypothesis**: Raw binary transfers over USB Bulk Endpoint 0x01 are framed with a fixed binary header (magic, message length, command ID) preceding the serialized Protobuf payload.
- **Supporting Leads**:
  - `udb_2026-11-10.log` logs raw messages with `track_id`, `version: 1`, `msg_size`.
  - `Tryx.KanaliProtocol.HeaderPb` and `Tryx.USBProtocol` packages define message headers.
- **Validation Required**:
  - Static disassembly / inspection of `UDB.exe`'s USB transmit loop or live packet capture of `TryxBridge` and USB.

## HYP-003: Media Storage vs. Live Streaming
- **Hypothesis**: Media (images, videos) are uploaded as files onto onboard flash storage (e.g. eMMC/NAND under `/userdata` or `/oem`) via `CMD_File_Transmit`, and the device renders them locally via an internal hardware H.264 video decoder loop, while dynamic hardware telemetry is updated in real time via lightweight Protobuf UI item update packets (`CMD_Update_UIItemText`).
- **Supporting Leads**:
  - `store.json` references files stored by filename (`download_52.mp4.h264_2240x1080`).
  - `UDB.exe` contains `CMD_File_Transmit`, `CMD_Get_FileList`, `CMD_File_Remove`.
  - Live logs show that while playing video, only small 344-byte telemetry update packets (`CMD_Update_UIItemText` with CPU/GPU temperatures) are transmitted over USB every second.
- **Validation Required**:
  - Observe USB transfer sizes during idle video playback vs during a new media upload.
