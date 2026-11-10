# Lumien Dashboard Editor Architecture & Scene Model

## 1. Executive Summary

Lumien provides a **pixel-accurate drag-and-drop dashboard editor** that replaces KANALI's rigid, restricted template system.

### Key Architectural Advantages over KANALI
1. **Unrestricted Widget Count**: Overcomes KANALI's arbitrary 3-indicator Vue checkbox limit. Users can position arbitrary numbers of metrics, clocks, gauges, and text labels.
2. **Freeform 2D Canvas Positioning**: Eliminates KANALI's fixed 1-column / 2-column vertical stacking formula. Widgets can be placed anywhere in the 2240 × 1080 pixel space.
3. **Curved Display Awareness**: Visual guides delineate the front face ($X \in [0, 1120)$), the 90° curvature bend zone ($X \in [1060, 1180]$), and the side face ($X \in [1120, 2240]$).
4. **Clean Decoupling**: Strict separation between the interactive UI editor state (selection, zoom, grid, history) and the transport-independent hardware scene specification.

---

## 2. System Pipeline: Editor to Hardware

```mermaid
flowchart TD
    subgraph UI ["Lumien Frontend (Tauri v2 / React 19 + TypeScript)"]
        Editor["2D Canvas Editor (2240 × 1080 virtual raster)"]
        SceneModel["Lumien Scene JSON (Layers, Widgets, Bindings)"]
        EditorState["Editor Session State (Zoom, Pan, Guides, Selection)"]
        Editor -->|Mutates| SceneModel
        Editor -->|Maintains| EditorState
    end

    subgraph Compiler ["Lumien Scene Compiler (Rust Core)"]
        SceneModel -->|Compile| MediaTask["Transcode Background Media (libx264 Raw H.264)"]
        SceneModel -->|Compile| LayoutTask["Generate RunConfigPb (Tag 201 Layout)"]
        SceneModel -->|Map| SensorTask["Bind Hardware Telemetry Sensors"]
    end

    subgraph Transport ["Hardware Transport (lumien-device)"]
        MediaTask -->|FileTransmit* (Tags 400..402)| USB["USB Bulk OUT (EP 0x01)"]
        LayoutTask -->|RunConfigPb (Tag 201)| USB
        SensorTask -->|1 Hz BatchGroupLabelUpdatePb (Tag 300)| USB
    end

    subgraph Device ["TRYX Panorama SE (RK3568 SoC)"]
        USB --> VPU["Rockchip VPU (Hardware H.264 Video Plane)"]
        USB --> LVGL["LVGL Engine (Hardware GUI Overlay Plane)"]
    end
```

---

## 3. Transport-Independent Scene Specification

The scene specification is stored on disk as a human-readable JSON document (`lumien-scene.json`).

### 3.1 JSON Schema

```json
{
  "$schema": "https://lumien.dev/schema/v1/scene.json",
  "version": 1,
  "canvas": {
    "width": 2240,
    "height": 1080,
    "background": {
      "type": "video",
      "source": "assets/cyberpunk_loop.mp4",
      "loop": true,
      "color": "#000000"
    }
  },
  "widgets": [
    {
      "id": "cpu-temp-widget",
      "groupId": 100,
      "x": 60,
      "y": 80,
      "width": 480,
      "height": 180,
      "textAlign": "left",
      "lineGap": -10,
      "elements": [
        {
          "labelId": 101,
          "type": "static-text",
          "line": 1,
          "order": 0,
          "gapLeft": 12,
          "text": "CPU TEMP",
          "fontSize": 32,
          "fontFamily": "roboto-regular",
          "color": "#DCE4DC"
        },
        {
          "labelId": 102,
          "type": "metric-value",
          "line": 0,
          "order": 1,
          "gapLeft": 0,
          "metricBinding": "hardware.cpu.temperature",
          "fallbackText": "--",
          "fontSize": 140,
          "fontFamily": "roboto-regular",
          "color": "#00FFCC"
        },
        {
          "labelId": 103,
          "type": "unit-text",
          "line": 0,
          "order": 2,
          "gapLeft": 4,
          "text": "°C",
          "fontSize": 140,
          "fontFamily": "roboto-regular",
          "color": "#00FFCC"
        }
      ]
    },
    {
      "id": "gpu-usage-widget",
      "groupId": 200,
      "x": 1260,
      "y": 80,
      "width": 480,
      "height": 180,
      "textAlign": "left",
      "lineGap": -10,
      "elements": [
        {
          "labelId": 201,
          "type": "static-text",
          "line": 1,
          "order": 0,
          "gapLeft": 12,
          "text": "GPU LOAD",
          "fontSize": 32,
          "fontFamily": "roboto-regular",
          "color": "#DCE4DC"
        },
        {
          "labelId": 202,
          "type": "metric-value",
          "line": 0,
          "order": 1,
          "gapLeft": 0,
          "metricBinding": "hardware.gpu.load",
          "fallbackText": "--",
          "fontSize": 140,
          "fontFamily": "roboto-regular",
          "color": "#FF007F"
        },
        {
          "labelId": 203,
          "type": "unit-text",
          "line": 0,
          "order": 2,
          "gapLeft": 4,
          "text": "%",
          "fontSize": 140,
          "fontFamily": "roboto-regular",
          "color": "#FF007F"
        }
      ]
    }
  ]
}
```

---

## 4. TypeScript Interface Definitions

```typescript
export type MetricId =
  | 'hardware.cpu.temperature'
  | 'hardware.cpu.load'
  | 'hardware.cpu.clock'
  | 'hardware.gpu.temperature'
  | 'hardware.gpu.load'
  | 'hardware.gpu.vram_used'
  | 'hardware.ram.load'
  | 'hardware.ram.used'
  | 'system.time.clock'
  | 'system.time.date';

export interface CanvasConfig {
  width: 2240;
  height: 1080;
  background: {
    type: 'video' | 'image' | 'color';
    source?: string;
    loop: boolean;
    color: string;
  };
}

export interface WidgetElement {
  labelId: number;
  type: 'static-text' | 'metric-value' | 'unit-text';
  line: number;
  order: number;
  gapLeft: number;
  text?: string;
  metricBinding?: MetricId;
  fallbackText?: string;
  fontSize: number;
  fontFamily: string;
  color: string; // 24-bit RGB hex (e.g. #00FFCC)
}

export interface WidgetGroup {
  id: string;
  groupId: number;
  x: number;
  y: number;
  width: number;
  height: number;
  textAlign: 'left' | 'center' | 'right';
  lineGap: number;
  elements: WidgetElement[];
}

export interface LumienScene {
  version: 1;
  canvas: CanvasConfig;
  widgets: WidgetGroup[];
}
```

---

## 5. Mapping Scene Model to Protobuf Wire Structures

The compilation from `LumienScene` to Protobuf structures is a direct, lossless transformation:

| Scene JSON Property | Protobuf Field | Target Message | Notes |
| :--- | :--- | :--- | :--- |
| `widget.groupId` | `group_id` | `LabelGroupPb` | Unique uint32 group identifier |
| `widget.x` | `group_x` | `LabelGroupPb` | Bound to $[0, 2240]$ |
| `widget.y` | `group_y` | `LabelGroupPb` | Bound to $[0, 1080]$ |
| `widget.width` | `group_width` | `LabelGroupPb` | Bounding box pixel width |
| `widget.height` | `group_height` | `LabelGroupPb` | Bounding box pixel height |
| `widget.textAlign` | `text_align` | `LabelGroupPb` | `0 = Left`, `1 = Center`, `2 = Right` |
| `widget.lineGap` | `line_gap` | `LabelGroupPb` | Vertical spacing (default: `-10`) |
| `element.labelId` | `label_id` | `LabelPb` | Unique uint32 label identifier |
| `element.line` | `line` | `LabelPb` | Line row index (0 = primary, 1 = subtitle) |
| `element.order` | `order` | `LabelPb` | Order within row |
| `element.gapLeft` | `gap_left` | `LabelPb` | Padding / spacing in pixels |
| `element.fontFamily` | `text_font` | `LabelPb` | Font family (e.g. `"roboto-regular"`) |
| `element.fontSize` | `text_size` | `LabelPb` | Font size in pixels |
| `element.color` | `text_color` | `LabelPb` | 24-bit integer RGB (e.g. `0x00FFCC` = 65484) |
| `element.text` | `text` | `LabelPb` | Initial display string |

### Runtime Telemetry Deltas
At runtime, only elements with `metricBinding` are queried. Every 1000 ms:
1. The host telemetry collector samples current hardware readings.
2. Formats readings into localized strings (e.g. `"48"`).
3. Constructs a `BatchGroupLabelUpdatePb` containing only modified labels.
4. Wire frame size: typically **< 40 bytes** per tick, consuming negligible USB bus bandwidth.
