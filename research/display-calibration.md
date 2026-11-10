# TRYX Panorama SE Display Calibration & Coordinate Mapping

## 1. Physical Panel Specifications

| Parameter | Specification | Verification Source |
| :--- | :--- | :--- |
| **Panel Type** | Curved AMOLED (L-shaped curvature) | TRYX Specifications / REA analysis |
| **Physical Resolution** | **2240 × 1080** pixels (Active Matrix) | `ScreenProfile` (`panorama2240`) |
| **Aspect Ratio** | ~2.074:1 (approx. 21:9 ultrawide) | 2240 / 1080 |
| **Refresh Rate** | 60 Hz | Rockchip VDec / VPSS subsystem |
| **Color Depth** | 24-bit RGB (8 bits per channel, BT.709) | H.264 Main Profile / YUV420p Full Range |
| **Curvature Form Factor** | Single continuous bend (~90° corner) | Physical hardware inspection |

---

## 2. Canvas Coordinate Geometry

The display coordinate origin $(0, 0)$ is located at the **top-left corner** of the unrolled 2240 × 1080 raster canvas.

```
(0, 0)                                 (1120, 0)                             (2240, 0)
┌─────────────────────────────────────────┬─────────────────────────────────────────┐
│                                         │                                         │
│                                         │                                         │
│            FRONT FACE                   │                SIDE FACE                │
│         (1120 × 1080 px)                │            (1120 × 1080 px)             │
│                                         │                                         │
│                                         │                                         │
└─────────────────────────────────────────┴─────────────────────────────────────────┘
(0, 1080)                              (1120, 1080)                          (2240, 1080)
                     ▲                                         ▲
                     │                                         │
            X in [0..1120]                           X in [1120..2240]
                                  ▲
                                  │
                       Curvature Bend Transition Zone
                            X in [1060..1180]
```

### 2.1 Display Zones & Panel Mapping

1. **Front Face ($X \in [0, 1120)$)**:
   - Primary flat display area facing directly toward the user / front PC case glass.
   - Ideal zone for primary metrics, gauges, clocks, and focal points.
2. **Curvature Bend Zone ($X \in [1060, 1180]$)**:
   - The panel curves smoothly across an approximate 90-degree radius around $X = 1120$.
   - **Optical Distortion Note**: Text placed directly across this boundary experiences optical elongation and radial foreshortening when viewed from an angle.
   - **Design Guideline**: Lumien's canvas editor provides a toggleable vertical guide at $X = 1120 \pm 60\text{ px}$ to warn users when widgets intersect the bend zone.
3. **Side Face ($X \in [1120, 2240]$)**:
   - Secondary flat display area wrapping around the side of the liquid cooler pump block.
   - Visible through side chassis panels or panoramic dual-glass cases.
   - Ideal for auxiliary telemetry (GPU clock, VRAM, fan speeds, secondary branding, animated accents).

---

## 3. Safe Margins & Border Clipping

From KANALI's layout generator decompilation (`ScreenService.initTextItemId` in `out/main/index-1c57b0ec.js`):
- **Horizontal Margins**:
  - Left margin: $X_{\min} = 60\text{ px}$.
  - Usable width: $W = 2120\text{ px}$.
  - Right margin: $2240 - (60 + 2120) = 60\text{ px}$.
- **Vertical Margins**:
  - Top safe margin: $Y \ge 40\text{ px}$.
  - Bottom safe margin: $Y + H \le 1040\text{ px}$ (40 px bottom margin).
- **Physical Bezel Occlusion**:
  - The Panorama SE bezel covers approximately 4–6 px along the outer perimeter.
  - Positioning widgets with a minimum 40 px margin ensures complete visibility from typical viewing angles.

---

## 4. Hardware Display Modes & Orientation Transforms

The display orientation is configured via `UserConfigPb.display_config` (`rotate`) and `UserConfigPb.work_config` (`media_mode`):

| Mode Name | Canvas Mapping | Hardware Rotation | Typical Pump Orientation |
| :--- | :--- | :--- | :--- |
| **Full Screen (Standard)** | $2240 \times 1080$ single canvas | `rotate = 270` | Standard vertical pump mount; single continuous video or canvas. |
| **Waterfall Mode** | $2240 \times 1080$ single canvas | `rotate = 90`, media `rotate = 180` | Inverted or horizontal pump mount; media rotated 180° for gravity-correct orientation. |
| **Split Screen (Dual)** | Two independent $1120 \times 1080$ viewports | `rotate = 270` | Left half runs video A; Right half runs video B; single LVGL overlay across both. |

---

## 5. Visual Calibration Artifact

To calibrate pixel alignment, aspect ratio scaling, and curved surface mapping on live hardware, Lumien includes a reference test pattern:
- **Source**: `scratch/calibration_2240x1080.png` (2240 × 1080, 24-bit RGB).
- **Transcoded Target**: `scratch/calibration_2240x1080.mp4.h264_2240x1080` (Annex B Raw H.264, 20,587 bytes).
- **Features**:
  - Outer 4-pixel border for bezel occlusion detection.
  - 100-pixel Cartesian grid with coordinate labels at 200 px intervals.
  - High-contrast split line at $X = 1120$ with labeled left/right halves.
  - Corner alignment markers $(0, 0)$, $(2240, 0)$, $(0, 1080)$, $(2240, 1080)$.
  - Curvature transition boundaries marked at $X = 1060$ and $X = 1180$.
