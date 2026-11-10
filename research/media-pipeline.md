# TRYX Panorama SE Media Pipeline Specification

## 1. Architectural Discovery: Media Representation

Reverse engineering of KANALI's media ingestion pipeline (`app.asar:out/main/index-1c57b0ec.js` and `MediaX.dll`) revealed a fundamental hardware constraint:

> **The TRYX Panorama SE display firmware does NOT possess a native static-image (JPEG/PNG/BMP) display mode.**
> All visual content—whether static images, animated GIFs, or videos—is transcoded into an **Annex B Raw H.264 video elementary stream** before transmission to the hardware.

### Media Processing Flow

```mermaid
flowchart TD
    In[User Input: PNG, JPG, GIF, MP4] --> Ingest[KANALI Media Service / Lumien]
    Ingest --> Transcode[Transcoder: libx264 / FFmpeg / MediaX.dll]
    Transcode -->|Profile: Main, Level: 4.1, Format: yuv420p| RawH264["Raw H.264 Elementary Stream (.mp4.h264_2240x1080)"]
    RawH264 --> USBTrans[USB File Transfer Pipeline]
    USBTrans -->|FileTransmitBeginPb (400)| DevStorage["Device eMMC (/userdata/default/)"]
    USBTrans -->|FileTransmitDataPb (401) in 256KB chunks| DevStorage
    USBTrans -->|FileTransmitEndPb (402) with CRC32| DevStorage
    DevStorage --> Activate["UserConfigPb (work_config.single_mode_media_file)"]
    Activate --> HardwareVDec["Rockchip RK3568 Hardware VPU Decoder (VDec -> VPSS -> VO)"]
```

---

## 2. H.264 Video Encoding Specifications

Recovered from FFprobe stream inspection of active device assets (`download_43.mp4.h264_2240x1080`) and verified via live transcode tests with `MediaX.dll` and standard FFmpeg:

| Parameter | Value | Verification Source |
| :--- | :--- | :--- |
| **Container Format** | **Raw H.264 Annex B** (no MP4/MKV container encapsulation) | FFprobe: `Input #0, h264`, `is_avc = false` |
| **Start Codes** | 4-byte `0x00, 0x00, 0x00, 0x01` | Byte inspection |
| **Resolution** | **2240 × 1080** pixels (Full) or **1120 × 1080** (Split) | SysConfig `vdec_param.width/height` |
| **Video Codec** | H.264 / AVC (MPEG-4 Part 10) | `libx264` |
| **Profile** | **Main** (`profile=Main`) | FFprobe / `MediaX.dll` |
| **Level** | **4.1** (`level=41`) | FFprobe / `MediaX.dll` |
| **Pixel Format** | `yuv420p` (or `yuvj420p` full range 0..255) | FFprobe: `pix_fmt=yuvj420p` |
| **Color Space / Matrix** | BT.709 (`color_space=bt709`, `color_range=pc`) | FFprobe |
| **Static Image Encoding** | 1-second single-frame or 25-frame GOP ($I$-frame only) | `MediaX.dll` log: `frame I:1 Avg QP:13.28` |
| **Video Framerate** | 60 fps (Full Screen 2240 × 1080) / 30 fps (1:1 Cropped) | `app.asar:uploadVideoFile` |
| **Standard File Suffix** | `.h264_2240x1080` (or `.h264_1120x1080`) | KANALI file naming convention |

### Standalone FFmpeg Command (Zero Proprietary DLL Dependency)
Lumien can generate bitstream-compatible assets using any standard in-box or bundled FFmpeg build:

```bash
ffmpeg -y -loop 1 -i input_canvas.png \
  -c:v libx264 \
  -profile:v main \
  -level 4.1 \
  -pix_fmt yuv420p \
  -t 1 \
  -f h264 \
  output.h264_2240x1080
```

---

## 3. Wire Protocol: File Transmit Pipeline (`usb_protocol.proto`)

All file transfers operate over Bulk OUT (EP `0x01`) and Bulk IN (EP `0x81`) using symmetrical `"TRYX"` 8-byte framing.

### 3.1 Step 1: Transfer Initiation (`FileTransmitBeginPb`)
Host initiates a session by sending tag 400:

```protobuf
message FileTransmitBeginPb {
  bytes file_name = 1;               // Destination filename (ASCII string as bytes)
  uint32 file_size = 2;              // Exact file size in bytes
}
```

Device response: `RspPackagePb` tag 800 (`file_transmit_begin_status`):
```protobuf
message FileTransmitStatusPb {
  enum enFileTransStatus {
    Success = 0;
    Failed = 1;
  }
  enFileTransStatus file_trans_status = 1;
}
```

### 3.2 Step 2: Data Streaming (`FileTransmitDataPb`)
Host streams data in ~256 KB chunks using tag 401:

```protobuf
message FileTransmitDataPb {
  bytes file_data = 1;               // Raw file chunk payload
}
```

Device response: `RspPackagePb` tag 801 (`file_transmit_data_status`): status 0 acknowledges receipt of the chunk.

### 3.3 Step 3: Transfer Finalization (`FileTransmitEndPb`)
Host completes transfer with tag 402:

```protobuf
message FileTransmitEndPb {
  bytes file_type = 1;               // ASCII "media" or "tmp"
  uint32 CRC = 2;                    // Standard IEEE 802.3 CRC32 of complete file
}
```

Device response: `RspPackagePb` tag 802 (`file_transmit_end_status`): status 0 confirms successful disk commit to `/userdata/default/<file_name>`.

### 3.4 Transfer Performance
In live trace on 2026-11-10:
- File size: 902,009 bytes (902 KB).
- Chunks: 4 data frames (~225 KB/chunk).
- Transfer time: **135 milliseconds** total.
- Effective transfer throughput: **~6.68 MB/s** over USB Bulk 2.0.

---

## 4. Playback Activation (`UserConfigPb.work_config`)

Once a file is stored on the device, it is activated for playback by sending a `UserConfigPb` (tag 200) command targeting `work_config` (tag 3):

```protobuf
message WorkConfigPb {
  enum enMediaMode {
    MediaMode_Single = 0;
    MediaMode_Dual = 1;
    MediaMode_Kaleidoscope = 2;
  }
  enum enLoopMode {
    LoopMode_Single = 0;
    LoopMode_MediaAll = 1;
    LoopMode_MediaRand = 2;
  }

  enMediaMode media_mode = 1;        // 0 = Full screen single video
  enLoopMode loop_mode = 2;          // 0 = Single video loop
  string single_mode_media_file = 3; // e.g. "calibration_2240x1080.mp4.h264_2240x1080"
}
```

Because of Protobuf merge semantics (verified at `UDB.exe` RVA `0x4b350`), this command immediately switches the active video loop while leaving display backlight, rotation, and overlay widgets undisturbed.
