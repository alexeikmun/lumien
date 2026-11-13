import { useRef, useState, useEffect, useMemo, useReducer, type CSSProperties } from 'react';
import { listen } from '@tauri-apps/api/event';
import coolingDelivery from './assets/presets/default_01.png';
import migration from './assets/presets/default_02.png';
import quantumTimeCapsule from './assets/presets/default_03.png';
import exoEcologies from './assets/presets/default_04.png';
import { createRoot } from 'react-dom/client';
import {
  sample,
  emptyScene,
  createWidget,
  geometry,
  logical,
  parseScene,
  kinds,
  profile,
  fontFamilies,
  getCssFontFamily,
  type Scene,
  type Widget,
  type Kind,
} from './scene';
import { device } from './adapter';
import { invoke } from '@tauri-apps/api/core';
import { historyReducer, initialHistory, withTelemetry, moveWidgets, takeStartupScene } from './editor';
import { sensors, isNetwork, metricMax, formatMetric } from './metrics';
import { Icon, sensorIcon } from './icons';
import './style.css';

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

function fontStyleOf(w: Widget) {
  const fam = (w.fontFamily || 'roboto').toLowerCase();
  const isRoboto = fam === 'roboto' || fam === 'roboto-black';
  return {
    fontFamily: getCssFontFamily(w.fontFamily),
    fontWeight: fam === 'roboto-black' ? 900 : isRoboto && w.bold ? 700 : 400,
    fontStyle: isRoboto && w.italic ? 'italic' : 'normal',
  } as const;
}

// Mirrors apply_scene's LabelGroup: value + unit on the primary row, title on the subtitle row,
// one text color, no widget background, and the same size multipliers/clamps the LCD uses.
function Preview({ w, readings, cpuTempStatus }: { w: Widget; readings: Record<string, number | null>; cpuTempStatus: string | null }) {
  const font = fontStyleOf(w);
  if (w.kind === 'Text') {
    return <div className="widget text" style={{ ...font, color: w.color, fontSize: w.fontSize }}>{w.text}</div>;
  }
  const key = sensors.find(s => s.binding === w.binding)?.key;
  const available = !isNetwork(w.binding) || (key && readings[key] != null);
  const isCpuTemp = w.binding === 'CPU temperature';
  const value = isCpuTemp ? readings.cpu_temp : available ? w.value : null;
  const metric = formatMetric(w.binding, value);
  const valueSize = clamp(Math.round(w.fontSize * 1.65), 16, 220);
  return (
    <div className="widget" style={{ ...font, color: w.color }} title={isCpuTemp && value == null ? cpuTempStatus ?? undefined : undefined}>
      <div className="metric-row">
        <span style={{ fontSize: valueSize }}>{metric.text}</span>
        <span style={{ fontSize: clamp(Math.round(valueSize * 0.65), 12, 140) }}>{metric.unit}</span>
      </div>
      <div className="metric-title" style={{ fontSize: clamp(Math.round(w.fontSize * 0.45), 14, 60) }}>{w.text}</div>
    </div>
  );
}

// Typing stays local until blur/Enter so clamping never fights the keyboard; valid values preview live.
function NumberField({ label, value, min, max, step, disabled, onBegin, onPreview, onCommit }: {
  label: string; value: number; min: number; max: number; step?: number | 'any'; disabled?: boolean;
  onBegin: () => void; onPreview: (v: number) => void; onCommit: (v?: number) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const finish = () => {
    if (draft === null) return onCommit();
    const n = Number(draft);
    setDraft(null);
    onCommit(draft.trim() !== '' && Number.isFinite(n) ? clamp(n, min, max) : undefined);
  };
  return (
    <label className="field">
      {label}
      <input type="number" value={draft ?? String(value)} min={min} max={max} step={step ?? 1} disabled={disabled}
        onFocus={onBegin}
        onChange={e => {
          setDraft(e.target.value);
          const n = e.target.valueAsNumber;
          if (Number.isFinite(n) && n >= min && n <= max) onPreview(n);
        }}
        onBlur={finish}
        onKeyDown={e => { if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur(); }} />
    </label>
  );
}

export type PresetItem = {
  id: string;
  name: string;
  file_name: string;
  thumbnail: string;
  is_builtin: boolean;
  is_active: boolean;
};

const builtinThumbnails = new Map([
  ['default_01.mp4.h264_2240x1080', coolingDelivery],
  ['default_02.mp4.h264_2240x1080', migration],
  ['default_03.mp4.h264_2240x1080', quantumTimeCapsule],
  ['default_04.mp4.h264_2240x1080', exoEcologies],
]);

function withPresetThumbnails(items: PresetItem[]) {
  return items.map(item => ({
    ...item, thumbnail: builtinThumbnails.get(item.file_name) ?? item.thumbnail,
  }));
}

const SOLID_SWATCHES = [
  { name: 'Pure Black', hex: '#000000', note: 'AMOLED Off' },
  { name: 'Charcoal', hex: '#0c151d', note: 'Lumien Slate' },
  { name: 'Surface', hex: '#18262e', note: 'Widget Deep' },
  { name: 'Cyber Navy', hex: '#0a192f', note: 'Dark Blue' },
  { name: 'Emerald', hex: '#062826', note: 'Deep Green' },
  { name: 'Indigo', hex: '#1f1638', note: 'Cyber Purple' },
  { name: 'Burgundy', hex: '#2b0b18', note: 'Deep Red' },
  { name: 'Pure White', hex: '#ffffff', note: 'Bright Light' },
  { name: 'Red', hex: '#ef4444', note: 'Warm red' },
  { name: 'Orange', hex: '#f97316', note: 'Orange' },
  { name: 'Gold', hex: '#eab308', note: 'Golden yellow' },
  { name: 'Green', hex: '#22c55e', note: 'Fresh green' },
  { name: 'Cyan', hex: '#06b6d4', note: 'Cyan blue' },
  { name: 'Blue', hex: '#3b82f6', note: 'Bright blue' },
  { name: 'Violet', hex: '#8b5cf6', note: 'Violet' },
  { name: 'Pink', hex: '#ec4899', note: 'Pink' },
];

function normalizeHex(hex: string): string {
  let clean = hex.trim();
  if (!clean.startsWith('#')) clean = '#' + clean;
  if (/^#[0-9a-fA-F]{6}$/.test(clean)) return clean.toLowerCase();
  if (/^#[0-9a-fA-F]{3}$/.test(clean)) {
    const r = clean[1], g = clean[2], b = clean[3];
    return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
  }
  return '#000000';
}

function toneOf(status: string): 'ok' | 'error' | 'busy' | 'info' {
  if (/…|\.\.\.$/.test(status)) return 'busy';
  if (/fail|error|could not|cannot|invalid|unsupported|not supported|cancelled|unavailable|limit/i.test(status)) return 'error';
  if (/applied|saved|activated|imported|removed|opened|ready|cleared|reopened|success/i.test(status)) return 'ok';
  return 'info';
}

const load = (key: string) => { try { return localStorage.getItem(key); } catch { return null; } };
const readStored = <T extends string>(key: string, allowed: readonly T[], fallback: T): T => {
  const v = load(key) as T | null;
  return v && allowed.includes(v) ? v : fallback;
};
const store = (key: string, value: string) => { try { localStorage.setItem(key, value); } catch { /* storage unavailable */ } };

type DownloadedVideo = { id: string; title: string; duration: number | null; bytes: number; thumbnail: string };
type LayoutOption = { path: string; name: string };
type BgTab = 'presets' | 'solid' | 'video';
const isLibraryVideo = (filename?: string) => /^(?:youtube_[\w-]{11}|local_\d{1,39})\.mp4\.h264_2240x1080$/.test(filename ?? '');

function withBackgroundFile(layout: Scene, filename: string): Scene {
  const solid = /^solid_([0-9a-f]{6})\.mp4\.h264_2240x1080$/i.exec(filename);
  return solid
    ? { ...layout, bgType: 'solid', bgColor: `#${solid[1].toLowerCase()}`, bgFile: undefined }
    : { ...layout, bgType: 'preset', bgFile: filename };
}

function App() {
  const desktop = '__TAURI_INTERNALS__' in window;
  const [editorActive, setEditorActive] = useState(false);
  const [youtubeUrl, setYoutubeUrl] = useState('');
  const [youtubeBusy, setYoutubeBusy] = useState(false);
  const [downloadedVideos, setDownloadedVideos] = useState<DownloadedVideo[]>([]);
  const [galleryError, setGalleryError] = useState('');
  const [galleryLoading, setGalleryLoading] = useState(false);
  const [playingVideo, setPlayingVideo] = useState<string | null>(null);
  const [layouts, setLayouts] = useState<LayoutOption[]>([]);
  const [{ scene, past, future }, dispatch] = useReducer(historyReducer, undefined, () => initialHistory(sample()));
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set()),
    [saved, setSaved] = useState<string | null>(null),
    [filePath, setFilePath] = useState<string | null>(null),
    [fileBusy, setFileBusy] = useState(desktop),
    [starting, setStarting] = useState(desktop),
    [fileError, setFileError] = useState<string | null>(null),
    [readings, setReadings] = useState<Record<string, number | null>>({}),
    [cpuTempStatus, setCpuTempStatus] = useState<string | null>(null),
    [pending, setPending] = useState<'new' | { scene: Scene; path: string } | null>(null),
    [zoom, setZoom] = useState(1),
    [fit, setFit] = useState(0.4),
    [pan, setPan] = useState({ x: 0, y: 0 }),
    [isPanning, setIsPanning] = useState(false),
    [status, setStatus] = useState('Ready'),
    [busy, setBusy] = useState(false),
    [activeTab, setActiveTab] = useState<'widgets' | 'backgrounds'>('widgets'),
    [bgSubtab, setBgSubtab] = useState<BgTab>(() => readStored('lumien_bg_subtab', ['presets', 'solid', 'video'] as const, 'presets')),
    [solidColor, setSolidColor] = useState<string>(() => normalizeHex(load('lumien_solid_color') ?? '#000000')),
    [hexInput, setHexInput] = useState<string>(() => solidColor.toUpperCase()),
    [presets, setPresets] = useState<PresetItem[]>([]),
    [presetsLoading, setPresetsLoading] = useState(false),
    [importing, setImporting] = useState(false),
    [activePresetFile, setActivePresetFile] = useState<string | null>(null),
    [previewBackground, setPreviewBackground] = useState<string | null>(null),
    [showCanvasBg, setShowCanvasBg] = useState(true),
    [deviceState, setDeviceState] = useState<{ connected: boolean; message: string }>({
      connected: false,
      message: 'Checking device status...',
    });

  const chooseBgTab = (tab: BgTab) => { setBgSubtab(tab); store('lumien_bg_subtab', tab); };

  useEffect(() => {
    if (!desktop || activeTab !== 'backgrounds' || bgSubtab !== 'video' || youtubeBusy) return;
    let active = true;
    setGalleryLoading(true); setGalleryError('');
    invoke<DownloadedVideo[]>('list_downloaded_videos')
      .then(videos => { if (active) setDownloadedVideos(videos); })
      .catch(error => { if (active) setGalleryError(String(error)); })
      .finally(() => { if (active) setGalleryLoading(false); });
    return () => { active = false; };
  }, [desktop, activeTab, bgSubtab, youtubeBusy]);

  useEffect(() => {
    if (desktop) invoke<LayoutOption[]>('list_layouts').then(setLayouts).catch(error => setStatus(String(error)));
  }, [desktop]);

  const renameStart = useRef<Scene | null>(null);
  const colorStart = useRef<Scene | null>(null);
  const editStart = useRef<{ scene: Scene; id: string } | null>(null);
  const discardDialog = useRef<HTMLDialogElement>(null);
  const fileOperation = useRef(false);
  const startupApply = useRef<Scene | null>(null);
  const viewport = useRef<HTMLDivElement>(null),
    canvas = useRef<HTMLDivElement>(null);
  const gesture = useRef<{
    scene: Scene; next: Scene; id: string; point: { x: number; y: number }; pointerId: number; moveIds?: ReadonlySet<string>;
  } | null>(null),
    panning = useRef<{ x: number; y: number; px: number; py: number } | null>(null);

  const selection = scene.widgets.filter(v => selected.has(v.id));
  const sceneJson = useMemo(() => JSON.stringify(scene), [scene]);
  const sensorSignature = useMemo(() => sensors.map(s => Number(scene.widgets.some(
    w => w.visible && w.kind === 'Metric value' && w.binding === s.binding,
  ))).join(''), [scene.widgets]);
  const pollDeviceStatus = !desktop || editorActive || (!starting && startupApply.current !== null);
  const w = selection.length === 1 ? selection[0] : undefined,
    scale = fit * zoom,
    dirty = !starting && saved !== sceneJson,
    previewScene = useMemo(() => withTelemetry(scene, readings), [scene, readings]),
    grid = scene.grid ?? true,
    snap = scene.snap ?? false,
    division = scene.split ?? true,
    bend = scene.bend ?? true,
    tone = toneOf(status);

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    fileOperation.current = true;
    invoke<{ path: string; content: string } | null>('restore_layout')
      .then(async (file) => {
        const activeFile = await invoke<string | null>('get_active_preset');
        if (!active) return;
        if (file || activeFile) {
          const layout = file ? parseScene(file.content) : sample();
          const restored = activeFile ? withBackgroundFile(layout, activeFile) : layout;
          if (file && restored.widgets.some(w => w.visible && w.kind === 'Metric value' && w.binding === 'CPU temperature')) {
            void invoke('warm_cpu_temperature').catch(() => {});
          }
          if (file) replaceDocument(restored, file.path, false);
          else dispatch({ type: 'replace', scene: restored });
          startupApply.current = restored;
          if (isLibraryVideo(restored.bgFile) && restored.bgType !== 'solid') {
            setBgSubtab('video');
          } else if (restored.bgType === 'solid') {
            setBgSubtab('solid');
            if (restored.bgColor) {
              setSolidColor(restored.bgColor);
              setHexInput(restored.bgColor.toUpperCase());
            }
          }
          if (file) setStatus('Reopened ' + file.path);
        }
      })
      .catch((e) => { if (active) setStatus('Could not reopen last layout: ' + String(e)); })
      .finally(() => {
        if (active) {
          fileOperation.current = false;
          setFileBusy(false);
          setStarting(false);
        }
      });
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!desktop) return;
    setPresetsLoading(true);
    invoke<PresetItem[]>('list_presets')
      .then(items => setPresets(withPresetThumbnails(items)))
      .catch((err) => setStatus('Could not load background presets: ' + String(err)))
      .finally(() => setPresetsLoading(false));
  }, [desktop]);

  useEffect(() => {
    if (scene.bgType === 'solid') {
      setSolidColor(scene.bgColor ?? '#000000');
      setHexInput((scene.bgColor ?? '#000000').toUpperCase());
      return;
    }
    if (scene.bgFile) {
      const preset = presets.find((item) => item.file_name === scene.bgFile);
      const video = downloadedVideos.find((item) => item.id.endsWith('/' + scene.bgFile));
      setActivePresetFile(scene.bgFile);
      setPreviewBackground(preset?.thumbnail ?? video?.thumbnail ?? '');
      if (!preset && !isLibraryVideo(scene.bgFile) && !presetsLoading && presets.length) setStatus(`Background preview unavailable for saved media: ${scene.bgFile}. Apply will still send its saved filename to the LCD.`);
      return;
    }
    const active = presets.find((item) => item.is_active);
    if (active) {
      setActivePresetFile(active.file_name);
      setPreviewBackground(active.thumbnail);
    }
  }, [scene.bgType, scene.bgColor, scene.bgFile, presets, presetsLoading, downloadedVideos]);

  // Runs a device operation with the shared busy lock so double clicks never interleave USB uploads.
  async function exclusive(task: () => Promise<void>) {
    if (busy) return;
    startupApply.current = null;
    setBusy(true);
    try { await task(); } finally { setBusy(false); }
  }

  const importPresets = async () => {
    if (!desktop || importing) return;
    setImporting(true);
    setStatus('Importing presets from KANALI…');
    try {
      const list = withPresetThumbnails(await invoke<PresetItem[]>('import_kanali_presets'));
      setPresets(list);
      const active = list.find((p) => p.is_active);
      if (active) {
        setActivePresetFile(active.file_name);
        setPreviewBackground(active.thumbnail);
      }
      setStatus(`Imported ${list.length} presets from KANALI.`);
    } catch (err) {
      setStatus(`Import failed: ${err}`);
    } finally {
      setImporting(false);
    }
  };

  const applyPreset = (p: PresetItem) => exclusive(async () => {
    if (!desktop) return;
    commit({ ...scene, bgType: 'preset', bgFile: p.file_name });
    setStatus(`Applying background “${p.name}”…`);
    try {
      await invoke('set_display_background', { fileName: p.file_name });
      setActivePresetFile(p.file_name);
      setPreviewBackground(p.thumbnail);
      setPresets((prev) => prev.map((item) => ({ ...item, is_active: item.file_name === p.file_name })));
      setPlayingVideo(null);
      setStatus(`Background “${p.name}” applied.`);
    } catch (err) {
      setStatus(`Failed to apply background: ${err}`);
    }
  });

  // The native color input fires on every drag tick: preview while picking, commit one undo step on blur.
  const onColorPickerChange = (color: string) => {
    const norm = normalizeHex(color);
    setSolidColor(norm);
    setHexInput(norm.toUpperCase());
    store('lumien_solid_color', norm);
    colorStart.current ??= scene;
    dispatch({ type: 'preview', scene: { ...scene, bgType: 'solid', bgColor: norm, bgFile: undefined } });
  };

  const onColorPickerDone = () => {
    if (colorStart.current) dispatch({ type: 'commit', scene, before: colorStart.current });
    colorStart.current = null;
  };

  const onHexInputChange = (raw: string) => {
    setHexInput(raw.toUpperCase());
    let clean = raw.trim();
    if (!clean.startsWith('#')) clean = '#' + clean;
    if (/^#[0-9a-fA-F]{6}$/.test(clean)) {
      const norm = clean.toLowerCase();
      setSolidColor(norm);
      store('lumien_solid_color', norm);
      commit({ ...scene, bgType: 'solid', bgColor: norm, bgFile: undefined });
    }
  };

  const onSelectSwatch = (hex: string) => {
    const norm = normalizeHex(hex);
    setSolidColor(norm);
    setHexInput(norm.toUpperCase());
    store('lumien_solid_color', norm);
    commit({ ...scene, bgType: 'solid', bgColor: norm, bgFile: undefined });
  };

  const applySolidColor = (color: string) => exclusive(async () => {
    const norm = normalizeHex(color);
    setSolidColor(norm);
    setHexInput(norm.toUpperCase());
    store('lumien_solid_color', norm);
    const updatedScene: Scene = { ...scene, bgType: 'solid', bgColor: norm, bgFile: undefined };
    commit(updatedScene);
    setStatus(`Applying solid background ${norm.toUpperCase()}…`);
    try {
      if (desktop) {
        await invoke('set_solid_color_background', { color: norm });
        setActivePresetFile('');
        setPreviewBackground('');
      }
      setPlayingVideo(null);
      setStatus(`Solid background ${norm.toUpperCase()} applied.`);
    } catch (err) {
      setStatus(`Failed to apply solid color: ${err}`);
    }
  });

  const applyToDisplay = () => exclusive(async () => {
    setStatus('Applying layout to display…');
    try {
      await applyBackground(previewScene);
      setStatus(await device.applyScene(parseScene(JSON.stringify(previewScene))));
    } catch (e) {
      setStatus(String(e));
    }
  });

  const clearScreen = () => exclusive(async () => {
    try { setStatus(await device.clearDisplay()); } catch (e) { setStatus(String(e)); }
  });

  async function applyBackground(layout: Scene) {
    if (!desktop) return;
    if (layout.bgType === 'solid') {
      await invoke('set_solid_color_background', { color: layout.bgColor });
      setPlayingVideo(null);
    } else if (layout.bgFile) {
      await invoke('set_display_background', { fileName: layout.bgFile });
      setPlayingVideo(isLibraryVideo(layout.bgFile) ? layout.bgFile : null);
    }
  }

  const playVideo = (label: string, run: () => Promise<string>) => exclusive(async () => {
    setYoutubeBusy(true); setStatus(label);
    try {
      const filename = await run();
      dispatch({ type: 'video-background', filename });
      setPlayingVideo(filename);
      setStatus('Video playing as background.');
    }
    catch (error) { setStatus('Video playback failed: ' + String(error)); }
    finally { setYoutubeBusy(false); }
  });

  const importMp4 = () => exclusive(async () => {
    setYoutubeBusy(true);
    setStatus('Importing MP4 for the display…');
    try {
      const imported = await invoke<boolean>('import_mp4');
      setStatus(imported ? 'MP4 imported. Select it in the video library to play.' : 'Import cancelled.');
    } catch (error) {
      setStatus('MP4 import failed: ' + String(error));
    } finally {
      setYoutubeBusy(false);
    }
  });

  async function removeVideo(video: DownloadedVideo) {
    if (busy || !window.confirm(`Remove “${video.title}” from Lumien’s video library?`)) return;
    setBusy(true);
    try {
      await invoke('remove_downloaded_video', { id: video.id });
      setDownloadedVideos(items => items.filter(item => item.id !== video.id));
      if (video.id.endsWith('/' + playingVideo)) setPlayingVideo(null);
      setStatus('Video removed from the library.');
    } catch (error) { setStatus('Could not remove video: ' + String(error)); }
    finally { setBusy(false); }
  }

  useEffect(() => {
    const wasPending = startupApply.current !== null;
    const restored = takeStartupScene(startupApply, scene,
      desktop && !starting && !fileBusy && !pending && !busy && deviceState.connected);
    if (!restored) {
      if (wasPending && !startupApply.current) setStatus('Startup apply cancelled: layout changed. Use Apply to display when ready.');
      else if (startupApply.current && !starting && !deviceState.connected) setStatus('Layout restored. Waiting for the display to connect…');
      return;
    }
    setBusy(true);
    setStatus('Applying restored layout to display…');
    void (async () => {
      let backgroundError = '';
      try {
        try {
          await applyBackground(restored);
        } catch (e) {
          backgroundError = ' Background restore failed: ' + String(e);
        }
        const message = filePath
          ? await device.applyScene(parseScene(JSON.stringify(withTelemetry(restored, readings))))
          : 'Background restored.';
        setStatus(filePath ? 'Startup apply: ' + message + backgroundError : backgroundError.trim() || message);
      } catch (e) {
        setStatus('Startup apply failed: ' + String(e) + '. Use Apply to display to retry.');
      } finally { setBusy(false); }
    })();
  }, [desktop, starting, fileBusy, pending, busy, deviceState.connected, scene, filePath]);

  useEffect(() => {
    const r = new ResizeObserver(() => {
      if (viewport.current) {
        setFit(Math.min((viewport.current.clientWidth - 80) / 2240, (viewport.current.clientHeight - 80) / 1080));
      }
    });
    if (viewport.current) r.observe(viewport.current);
    return () => r.disconnect();
  }, []);

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    let request = 0;
    const refresh = async () => {
      const id = ++request;
      const shown = await invoke<boolean>('editor_active').catch(() => false);
      if (active && id === request) setEditorActive(shown);
    };
    const subscription = listen('editor-activity-changed', () => void refresh());
    void subscription.then(refresh, refresh);
    return () => {
      active = false;
      void subscription.then(unlisten => unlisten(), () => {});
    };
  }, [desktop]);

  // Hidden sign-in launches still need to find the display while a restore is pending.
  useEffect(() => {
    if (!pollDeviceStatus) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const check = async () => {
      const s = await device.getStatus();
      if (!active) return;
      setDeviceState(previous => previous.connected === s.connected && previous.message === s.message ? previous : s);
      if (desktop) timer = setTimeout(check, 3000);
    };
    void check();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [desktop, pollDeviceStatus]);

  useEffect(() => {
    if (!editorActive) return;
    const keys = sensors.filter((_, index) => sensorSignature[index] === '1').map(s => s.key);
    if (!keys.length) {
      setReadings(previous => Object.keys(previous).length ? {} : previous);
      setCpuTempStatus(null);
      return;
    }
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const { readings: telemetry, cpuTempStatus: tempStatus } = await device.getTelemetry(keys);
        if (!active) return;
        setReadings(previous => Object.keys(previous).length === keys.length
          && keys.every(key => previous[key] === telemetry[key]) ? previous : telemetry);
        setCpuTempStatus(tempStatus);
      } catch {
        // telemetry is best effort; widgets fall back to their saved values
      }
      if (active) timer = setTimeout(poll, 1500);
    };
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [editorActive, sensorSignature]);

  function commit(next: Scene) {
    dispatch({ type: 'commit', scene: next });
  }

  function select(id: string | null, toggle = false) {
    if (id === null || !toggle) {
      setSelected(new Set(id === null ? [] : [id]));
      return;
    }
    setSelected(previous => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  const withWidget = (base: Scene, id: string, p: Partial<Widget>): Scene =>
    ({ ...base, widgets: base.widgets.map((v) => (v.id === id ? geometry({ ...v, ...p }) : v)) });

  function patch(p: Partial<Widget>) {
    if (w) commit(withWidget(scene, w.id, p));
  }

  // Continuous inspector edits (typing, color dragging) preview live and land as one undo step on blur.
  function beginEdit() {
    if (w) editStart.current ??= { scene, id: w.id };
  }
  function previewEdit(p: Partial<Widget>) {
    if (!w) return;
    editStart.current ??= { scene, id: w.id };
    dispatch({ type: 'preview', scene: withWidget(scene, editStart.current.id, p) });
  }
  function endEdit(p?: Partial<Widget>) {
    const start = editStart.current;
    editStart.current = null;
    if (!start) { if (p) patch(p); return; }
    dispatch({ type: 'commit', scene: p ? withWidget(scene, start.id, p) : scene, before: start.scene });
  }
  const live = <K extends keyof Widget>(key: K) => ({
    onFocus: beginEdit,
    onChange: (e: { target: { value: string } }) => previewEdit({ [key]: e.target.value } as Partial<Widget>),
    onBlur: () => endEdit(),
  });
  const numberProps = (key: keyof Widget) => ({
    onBegin: beginEdit,
    onPreview: (v: number) => previewEdit({ [key]: v } as Partial<Widget>),
    onCommit: (v?: number) => endEdit(v === undefined ? undefined : { [key]: v } as Partial<Widget>),
  });

  function restoreHistory(type: 'undo' | 'redo', next?: Scene) {
    if (!next) return;
    dispatch({ type });
    setSelected(previous => new Set(next.widgets.filter(v => previous.has(v.id)).map(v => v.id)));
  }

  function undo() { restoreHistory('undo', past.at(-1)); }
  function redo() { restoreHistory('redo', future[0]); }

  function duplicate() {
    if (!selection.length) return;
    if (scene.widgets.length + selection.length > 100) {
      setStatus('Editor limit: 100 widgets');
      return;
    }
    const copies = selection.map(v => ({
      ...v, id: crypto.randomUUID(), name: v.name.slice(0, 115) + ' copy', locked: false,
    }));
    const ids = new Set(copies.map(v => v.id));
    commit(moveWidgets({ ...scene, widgets: [...scene.widgets, ...copies] }, ids, 20, 20));
    setSelected(ids);
  }

  function remove() {
    const widgets = scene.widgets.filter(v => !selected.has(v.id) || v.locked);
    if (widgets.length === scene.widgets.length) return;
    commit({ ...scene, widgets });
    setSelected(new Set(selection.filter(v => v.locked).map(v => v.id)));
  }

  function setSelectionFlag(key: 'visible' | 'locked', value: boolean) {
    commit({ ...scene, widgets: scene.widgets.map(v => selected.has(v.id) ? { ...v, [key]: value } : v) });
  }

  function insert(widget: Widget) {
    if (scene.widgets.length >= 100) {
      setStatus('Editor limit: 100 widgets');
      return;
    }
    commit({ ...scene, widgets: [...scene.widgets, widget] });
    select(widget.id);
  }

  function add(kind: Kind, x = 120, y = 120) {
    insert(createWidget(kind, x, y));
  }

  function addSensor(binding: string, name: string, text: string) {
    const offset = (scene.widgets.length % 6) * 30;
    insert(geometry({
      ...createWidget('Metric value', 120 + offset, 120 + offset),
      name,
      width: isNetwork(binding) ? 600 : 420,
      height: 220,
      text,
      value: 0,
      binding,
    }));
  }

  function point(x: number, y: number) {
    return logical(x, y, canvas.current!.getBoundingClientRect());
  }

  function order(delta: number) {
    if (!w) return;
    const items = [...scene.widgets],
      i = items.findIndex((v) => v.id === w.id),
      j = Math.max(0, Math.min(items.length - 1, i + delta));
    [items[i], items[j]] = [items[j], items[i]];
    commit({ ...scene, widgets: items });
  }

  function toggleWidget(id: string, key: 'visible' | 'locked') {
    commit({ ...scene, widgets: scene.widgets.map((a) => (a.id === id ? { ...a, [key]: !a[key] } : a)) });
  }

  function replaceDocument(next: Scene, path: string | null, remember = true) {
    startupApply.current = null;
    dispatch({ type: 'replace', scene: next });
    select(null);
    setFilePath(path);
    setSaved(JSON.stringify(next));
    setPan({ x: 0, y: 0 });
    setZoom(1);
    setStatus(path ? 'Layout opened' : 'New layout ready');
    if (desktop && path && remember) {
      void invoke('remember_layout', { path }).catch((e) => setStatus('Layout opened, but could not remember it for startup: ' + String(e)));
    }
  }

  function acceptOpen(next: Scene, path: string) {
    setFileError(null);
    if (dirty) setPending({ scene: next, path });
    else replaceDocument(next, path);
  }

  function newLayout() {
    setFileError(null);
    if (dirty) setPending('new');
    else replaceDocument(emptyScene(), null);
  }

  async function selectLayout(path: string) {
    if (!desktop || !path || fileOperation.current) return;
    fileOperation.current = true;
    setFileBusy(true);
    try {
      const file = await invoke<{ path: string; content: string }>('load_layout', { path });
      acceptOpen(await hydrateBackground(parseScene(file.content)), file.path);
    } catch (e) { setStatus(String(e)); }
    finally { fileOperation.current = false; setFileBusy(false); }
  }

  async function hydrateBackground(layout: Scene): Promise<Scene> {
    if (layout.bgType !== 'preset' || layout.bgFile) return layout;
    const activeFile = await invoke<string | null>('get_active_preset');
    return activeFile ? withBackgroundFile(layout, activeFile) : layout;
  }

  async function save(saveAs = false): Promise<boolean> {
    if (fileOperation.current) return false;
    setFileError(null);
    fileOperation.current = true;
    setFileBusy(true);
    try {
      const snapshot = parseScene(JSON.stringify(scene));
      const content = JSON.stringify(snapshot, null, 2);
      if (!desktop) {
        const url = URL.createObjectURL(new Blob([content], { type: 'application/json' }));
        const a = document.createElement('a');
        a.href = url;
        a.download = snapshot.name.replace(/[^a-z0-9_-]/gi, '_') + '.lumien.json';
        a.click();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        setStatus('Layout download requested. Desktop Save confirms disk writes.');
        return false;
      }
      const path = await invoke<string | null>('save_layout', { content, path: saveAs ? null : filePath });
      if (!path) { setStatus('Save cancelled'); return false; }
      setFilePath(path);
      setSaved(JSON.stringify(snapshot));
      invoke<LayoutOption[]>('list_layouts').then(setLayouts).catch(() => {});
      setStatus('Saved to ' + path);
      try { await invoke('remember_layout', { path }); }
      catch (e) { setStatus('Layout saved, but could not remember it for startup: ' + String(e)); }
      return true;
    } catch (e) { setStatus(String(e)); setFileError(String(e)); return false; }
    finally { fileOperation.current = false; setFileBusy(false); }
  }

  function continuePending() {
    if (!pending) return;
    if (pending === 'new') replaceDocument(emptyScene(), null);
    else replaceDocument(pending.scene, pending.path);
    setPending(null);
  }

  useEffect(() => {
    if (pending) discardDialog.current?.showModal();
    else discardDialog.current?.close();
  }, [pending]);

  useEffect(() => {
    if (!dirty) return;
    const warn = (e: BeforeUnloadEvent) => { e.preventDefault(); e.returnValue = ''; };
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [dirty]);

  useEffect(() => {
    function key(e: KeyboardEvent) {
      if (pending || fileBusy || gesture.current) return;
      const mod = e.ctrlKey || e.metaKey, k = e.key.toLowerCase();
      if (mod && (k === 's' || k === 'n')) {
        e.preventDefault();
        if (k === 's') void save(e.shiftKey);
        else newLayout();
        return;
      }
      if ((e.target as HTMLElement).closest('input,textarea,select,[contenteditable]')) return;
      if (mod && k === 'z') { e.preventDefault(); e.shiftKey ? redo() : undo(); return; }
      if (mod && k === 'y') { e.preventDefault(); redo(); return; }
      if (mod && k === 'd') { e.preventDefault(); duplicate(); return; }
      if (mod && (k === '=' || k === '+')) { e.preventDefault(); setZoom(z => Math.min(5, z * 1.25)); return; }
      if (mod && k === '-') { e.preventDefault(); setZoom(z => Math.max(0.25, z / 1.25)); return; }
      if (mod && k === '0') { e.preventDefault(); setZoom(1); setPan({ x: 0, y: 0 }); return; }
      if (e.key === 'Escape') { select(null); return; }
      if (!selection.length) return;
      if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); remove(); return; }
      const n = e.shiftKey ? 10 : 1;
      const d: Record<string, [number, number]> = {
        ArrowLeft: [-n, 0],
        ArrowRight: [n, 0],
        ArrowUp: [0, -n],
        ArrowDown: [0, n],
      };
      if (d[e.key]) {
        e.preventDefault();
        commit(moveWidgets(scene, selected, ...d[e.key]));
      }
    }
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });

  const docLocked = fileBusy || !!pending;
  const canvasBg = showCanvasBg && scene.bgType === 'solid' ? solidColor : '#000000';
  const fam = (w?.fontFamily || 'roboto').toLowerCase();
  const singleWeight = fam === 'orbitron' || fam === 'bitcount-single' || fam === 'rubik-glitch';

  return (
    <div className="app" inert={starting || youtubeBusy} aria-busy={starting || busy}>
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark"><Icon name="logo" size={17} /></span>
          Lumien
        </div>
        <div className="doc">
          <input
            aria-label="Layout name"
            className="doc-name"
            maxLength={120}
            value={scene.name}
            disabled={docLocked}
            title={filePath ?? 'Layout has not been saved to a file'}
            onFocus={() => { renameStart.current = scene; }}
            onChange={(e) => dispatch({ type: 'preview', scene: { ...scene, name: e.target.value } })}
            onBlur={() => {
              if (renameStart.current) dispatch({ type: 'commit', scene, before: renameStart.current });
              renameStart.current = null;
            }}
          />
          <span className={`save-state ${dirty ? 'dirty' : ''}`} role="status" aria-label="Layout save status" title={filePath ?? 'No file yet'}>
            {dirty ? 'Unsaved' : filePath ? 'Saved' : 'Not saved'}
          </span>
        </div>
        <div className="tool-group">
          <button className="ghost icon-btn" aria-label="New layout" disabled={docLocked} onClick={newLayout} title="New layout (Ctrl+N)"><Icon name="file" /></button>
          <select aria-label="Open saved layout" className="layout-picker" value={filePath ?? ''}
            disabled={docLocked || !layouts.length} onChange={e => void selectLayout(e.target.value)}>
            <option value="">{layouts.length ? 'Open layout…' : 'No saved layouts'}</option>
            {layouts.map(layout => <option key={layout.path} value={layout.path}>{layout.name}</option>)}
          </select>
          <button className="ghost" disabled={docLocked} onClick={() => void save()} title="Save (Ctrl+S) · Save as (Ctrl+Shift+S)">
            {fileBusy && !starting ? <span className="spinner" /> : <Icon name="save" />} Save
          </button>
        </div>
        <div className="tool-group">
          <button className="ghost icon-btn" aria-label="Undo" disabled={!past.length || docLocked} onClick={undo} title="Undo (Ctrl+Z)"><Icon name="undo" /></button>
          <button className="ghost icon-btn" aria-label="Redo" disabled={!future.length || docLocked} onClick={redo} title="Redo (Ctrl+Y)"><Icon name="redo" /></button>
        </div>
        <div className="spacer" />
        <div className={`device-chip ${deviceState.connected ? 'online' : ''}`} title={deviceState.message} role="status">
          <span className="dot" />{deviceState.connected ? 'Display connected' : 'No display'}
        </div>
        <button className="ghost" disabled={busy || !deviceState.connected} onClick={clearScreen} title="Remove all widgets from the LCD">
          <Icon name="screenOff" /> Clear
        </button>
        <button className="primary" disabled={busy} onClick={applyToDisplay} title="Send this layout and its background to the LCD">
          {busy ? <span className="spinner" /> : <Icon name="send" />}
          {busy ? 'Applying…' : 'Apply to display'}
        </button>
      </header>

      <dialog ref={discardDialog} className="document-dialog" aria-labelledby="unsaved-title" onCancel={(e) => { e.preventDefault(); if (!fileBusy) setPending(null); }}>
        <h2 id="unsaved-title">Save changes to “{scene.name || 'Untitled layout'}”?</h2>
        <p>Your changes will be lost if you don't save them.</p>
        {fileError && <p role="alert">{fileError}</p>}
        <div className="dialog-actions">
          <button className="ghost" autoFocus disabled={fileBusy} onClick={() => setPending(null)}>Cancel</button>
          <button className="danger" disabled={fileBusy} onClick={continuePending}>Don't save</button>
          <button className="primary" disabled={fileBusy} onClick={async () => { if (await save()) continuePending(); }}>Save</button>
        </div>
      </dialog>

      <div className="workspace">
        <aside className="sidebar left">
          <div className="segmented" role="tablist" aria-label="Sidebar">
            <button role="tab" aria-selected={activeTab === 'widgets'} onClick={() => setActiveTab('widgets')}>
              <Icon name="layers" size={14} /> Widgets
            </button>
            <button role="tab" aria-selected={activeTab === 'backgrounds'} onClick={() => setActiveTab('backgrounds')}>
              <Icon name="image" size={14} /> Background
            </button>
          </div>

          <div className="sidebar-scroll" role="tabpanel">
            {activeTab === 'widgets' ? (
              <>
                <section className="section">
                  <h2 className="section-title">Add to display</h2>
                  <div className="add-grid">
                    {kinds.map((kind) => (
                      <button key={kind} className="add-tile" draggable
                        onDragStart={(e) => e.dataTransfer.setData('lumien/widget', kind)}
                        onClick={() => add(kind)}>
                        <span className="tile-icon"><Icon name={kind === 'Text' ? 'text' : 'gauge'} size={17} /></span>
                        <strong>{kind === 'Text' ? 'Text' : 'Live metric'}</strong>
                        <small>{kind === 'Text' ? 'Title or label' : 'Sensor reading'}</small>
                      </button>
                    ))}
                  </div>
                  <p className="hint">Click to add, or drag onto the display.</p>
                </section>

                <section className="section">
                  <h2 className="section-title">Sensors <span className="count">{sensors.length}</span></h2>
                  <div className="sensor-list">
                    {sensors.map((s) => (
                      <button key={s.binding} className="sensor-btn" onClick={() => addSensor(s.binding, s.name, s.text)} title={`Add a ${s.name} widget`}>
                        <span className="sensor-icon"><Icon name={sensorIcon(s.binding)} size={15} /></span>
                        <span>{s.name}</span>
                        <span className="unit">
                          {s.key === 'cpu_temp'
                            ? (readings.cpu_temp != null ? `${readings.cpu_temp} °C` : '—')
                            : readings[s.key] != null && !isNetwork(s.binding) ? `${Math.round(readings[s.key]!)} ${formatMetric(s.binding, 0).unit}` : formatMetric(s.binding, null).unit}
                        </span>
                        <span className="add"><Icon name="plus" size={14} /></span>
                      </button>
                    ))}
                  </div>
                </section>

                <section className="section">
                  <h2 className="section-title">Layers <span className="count">{scene.widgets.length}</span></h2>
                  {scene.widgets.length === 0 && <p className="hint">Nothing on the display yet. Add a metric or text above.</p>}
                  <ul className="layer-list">
                    {[...scene.widgets].reverse().map((v) => (
                      <li key={v.id} className={`layer ${selected.has(v.id) ? 'active' : ''} ${v.visible ? '' : 'hidden'}`}>
                        <button className="layer-main" onClick={(e) => select(v.id, e.ctrlKey || e.metaKey)} aria-pressed={selected.has(v.id)}>
                          <Icon name={v.kind === 'Text' ? 'text' : sensorIcon(v.binding)} size={14} />
                          <span>{v.name}</span>
                        </button>
                        <button className="icon-btn" aria-label={(v.visible ? 'Hide ' : 'Show ') + v.name} title={v.visible ? 'Hide' : 'Show'}
                          onClick={() => toggleWidget(v.id, 'visible')}>
                          <Icon name={v.visible ? 'eye' : 'eyeOff'} size={14} />
                        </button>
                        <button className={`icon-btn ${v.locked ? 'on' : ''}`} aria-label={(v.locked ? 'Unlock ' : 'Lock ') + v.name} title={v.locked ? 'Unlock' : 'Lock'}
                          onClick={() => toggleWidget(v.id, 'locked')}>
                          <Icon name={v.locked ? 'lock' : 'unlock'} size={14} />
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              </>
            ) : (
              <section className="section">
                <div className="segmented bg-tabs" role="tablist" aria-label="Background type">
                  <button role="tab" aria-selected={bgSubtab === 'presets'} onClick={() => chooseBgTab('presets')}>
                    <Icon name="film" size={14} /> Animated {presets.length > 0 && <span className="count">{presets.length}</span>}
                  </button>
                  <button role="tab" aria-selected={bgSubtab === 'solid'} onClick={() => chooseBgTab('solid')}>
                    <Icon name="palette" size={14} /> Solid
                  </button>
                  <button role="tab" aria-selected={bgSubtab === 'video'} onClick={() => chooseBgTab('video')}>
                    <Icon name="video" size={14} /> Video
                  </button>
                </div>

                {bgSubtab === 'presets' && (
                  <>
                    <div className="row-between">
                      <span className="hint">Click a loop to show it on the display.</span>
                      <button className="ghost icon-btn" disabled={importing || !desktop} onClick={importPresets} title="Import presets from KANALI" aria-label="Import presets from KANALI">
                        {importing ? <span className="spinner" /> : <Icon name="download" />}
                      </button>
                    </div>
                    {presetsLoading ? (
                      <p className="hint">Loading presets…</p>
                    ) : presets.length === 0 ? (
                      <div className="empty-state">
                        <p>No animated backgrounds yet.</p>
                        <button disabled={importing || !desktop} onClick={importPresets}><Icon name="download" /> Import from KANALI</button>
                      </div>
                    ) : (
                      <div className="preset-list">
                        {presets.map((p) => {
                          const isActive = scene.bgType !== 'solid' && p.file_name === activePresetFile;
                          return (
                            <button key={p.id} className={`preset-card ${isActive ? 'active' : ''}`} disabled={busy || !desktop}
                              aria-pressed={isActive} onClick={() => void applyPreset(p)} title={`Show ${p.name} on the display`}>
                              {p.thumbnail ? <img className="thumb" src={p.thumbnail} alt="" /> : <span className="thumb empty">No preview</span>}
                              <span className="preset-meta">
                                <span>{p.name}</span>
                                {isActive ? <span className="tag live">On display</span>
                                  : <span className="tag"><span className="apply-hint">Apply · </span>{p.is_builtin ? 'Built-in' : 'Imported'}</span>}
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </>
                )}

                {bgSubtab === 'solid' && (
                  <div className="solid-panel">
                    <div className="color-preview">
                      <div className="swatch-large" style={{ backgroundColor: solidColor }} />
                      <div className="preset-meta">
                        <code className="color-field">{solidColor.toUpperCase()}</code>
                        {scene.bgType === 'solid' ? <span className="tag live">In layout</span> : <span className="tag">Preview</span>}
                      </div>
                    </div>
                    <div className="color-row">
                      <input
                        type="color"
                        aria-label="Choose background color"
                        value={solidColor}
                        onFocus={() => { colorStart.current = scene; }}
                        onChange={(e) => onColorPickerChange(e.target.value)}
                        onBlur={onColorPickerDone}
                        title="Open color picker"
                      />
                      <input
                        type="text"
                        aria-label="Background hex color"
                        value={hexInput}
                        onChange={(e) => onHexInputChange(e.target.value)}
                        onBlur={() => setHexInput(solidColor.toUpperCase())}
                        placeholder="#000000"
                        maxLength={7}
                        spellCheck={false}
                      />
                    </div>
                    <div className="swatches" role="group" aria-label="Preset colors">
                      {SOLID_SWATCHES.map((sw) => (
                        <button key={sw.hex} type="button" className="swatch" style={{ backgroundColor: sw.hex }}
                          aria-pressed={solidColor === sw.hex} aria-label={sw.name}
                          onClick={() => onSelectSwatch(sw.hex)} title={`${sw.name} · ${sw.note}`} />
                      ))}
                    </div>
                    <button className="primary block" disabled={busy} onClick={() => void applySolidColor(solidColor)}>
                      {busy ? <span className="spinner" /> : <Icon name="send" />} Show on display
                    </button>
                  </div>
                )}

                {bgSubtab === 'video' && (
                  <>
                    <div className="video-form">
                      <button className="block" disabled={!desktop || busy} onClick={() => void importMp4()}>
                        <Icon name="file" /> Import MP4
                      </button>
                      <p className="hint">Imports the first 5 minutes, without audio.</p>
                      <label className="field" htmlFor="youtube-url">
                        YouTube link
                        <input id="youtube-url" type="url" placeholder="https://www.youtube.com/watch?v=…" value={youtubeUrl} onChange={e => setYoutubeUrl(e.target.value)} />
                      </label>
                      <button className="primary block" disabled={!desktop || !deviceState.connected || busy || !youtubeUrl.trim()}
                        onClick={() => void playVideo('Downloading, converting and uploading the video…', () => invoke<string>('play_youtube', { url: youtubeUrl.trim() }))}>
                        <Icon name="play" /> Play as background
                      </button>
                      <p className="hint">Up to 5 minutes, silent, cropped to fill the panel behind your widgets.</p>
                    </div>
                    <h2 className="section-title" style={{ marginTop: 18 }}>Video library <span className="count">{downloadedVideos.length}</span></h2>
                    {galleryLoading && <p className="hint" role="status">Loading videos…</p>}
                    {galleryError && <p className="hint error" role="alert">{galleryError}</p>}
                    {!galleryLoading && !galleryError && downloadedVideos.length === 0 && <p className="hint">Import an MP4 or play a YouTube video to add it here.</p>}
                    <div className="video-list">
                      {downloadedVideos.map(video => {
                        const isPlaying = playingVideo !== null && video.id.endsWith('/' + playingVideo);
                        return (
                        <article key={video.id} className={`video-card ${isPlaying ? 'selected' : ''}`}>
                          <button type="button" className="video-play" disabled={busy || !deviceState.connected}
                            aria-pressed={isPlaying} title={`Play ${video.title} on the display`}
                            onClick={() => void playVideo('Uploading saved video to the display…', () => invoke<string>('play_downloaded_video', { id: video.id }))}>
                            {video.thumbnail ? <img className="thumb" src={video.thumbnail} alt="" /> : <span className="thumb empty"><Icon name="play" size={22} /></span>}
                            <span className="preset-meta">
                              <strong>{video.title}</strong>
                              <span className="tag">{video.duration !== null && `${Math.floor(video.duration / 60)}:${String(Math.floor(video.duration % 60)).padStart(2, '0')} · `}{(video.bytes / 1048576).toFixed(1)} MB{isPlaying ? ' · Playing' : ''}</span>
                            </span>
                          </button>
                          <button type="button" className="video-delete" disabled={busy}
                            aria-label={`Remove ${video.title} from the video library`} title="Remove video"
                            onClick={() => void removeVideo(video)}>
                            <Icon name="trash" size={15} />
                          </button>
                        </article>
                        );
                      })}
                    </div>
                  </>
                )}
              </section>
            )}
          </div>

          <div className="hardware-card">
            <strong>Panorama SE · 2240 × 1080</strong>
            {deviceState.message}
          </div>
        </aside>

        <section className="stage">
          <div className="stage-toolbar">
            <div className="stage-title">
              <span>Display</span> <b>2240 × 1080</b>
              <span className="zones">Front 1120 · Bend 1060–1180 · Side 1120</span>
            </div>
            <div className="toggle-group">
              {([
                ['grid', 'Grid', grid, 'grid'],
                ['snap', 'Snap', snap, 'magnet'],
                ['split', 'Split', division, 'split'],
                ['bend', 'Bend', bend, 'bend'],
              ] as const).map(([key, label, on, icon]) => (
                <button key={key} className="toggle" aria-pressed={on} title={`${label} guide`} onClick={() => commit({ ...scene, [key]: !on })}>
                  <Icon name={icon} size={14} /><span className="label">{label}</span>
                </button>
              ))}
              {(previewBackground || scene.bgType === 'solid') && (
                <button className="toggle" aria-pressed={showCanvasBg} title="Show the background on the canvas" onClick={() => setShowCanvasBg(v => !v)}>
                  <Icon name="image" size={14} /><span className="label">Background</span>
                </button>
              )}
            </div>
            <div className="zoom-group">
              <button className="ghost icon-btn" aria-label="Zoom out" title="Zoom out (Ctrl+−)" onClick={() => setZoom((z) => Math.max(0.25, z / 1.25))}><Icon name="minus" size={14} /></button>
              <span className="zoom-value">{Math.round(scale * 100)}%</span>
              <button className="ghost icon-btn" aria-label="Zoom in" title="Zoom in (Ctrl++)" onClick={() => setZoom((z) => Math.min(5, z * 1.25))}><Icon name="plus" size={14} /></button>
              <button className="ghost icon-btn" aria-label="Fit to window" title="Fit (Ctrl+0)" onClick={() => { setZoom(1); setPan({ x: 0, y: 0 }); }}><Icon name="fit" size={14} /></button>
            </div>
          </div>
          <div
            className={`viewport ${isPanning ? 'panning' : ''}`}
            ref={viewport}
            onWheel={(e) => setZoom((z) => clamp(z * (e.deltaY < 0 ? 1.1 : 1 / 1.1), 0.25, 5))}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault();
              const kind = e.dataTransfer.getData('lumien/widget') as Kind;
              if (kinds.includes(kind)) {
                const p = point(e.clientX, e.clientY);
                add(kind, p.x, p.y);
              }
            }}
            onPointerDown={(e) => {
              if (e.target === e.currentTarget || e.button === 1 || e.altKey) {
                e.preventDefault();
                panning.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
                setIsPanning(true);
                e.currentTarget.setPointerCapture(e.pointerId);
                if (e.target === e.currentTarget) select(null);
              }
            }}
            onPointerMove={(e) => {
              if (panning.current) {
                const p = panning.current;
                setPan({ x: p.px + e.clientX - p.x, y: p.py + e.clientY - p.y });
                return;
              }
              const g = gesture.current;
              if (!g || g.pointerId !== e.pointerId) return;
              const p = point(e.clientX, e.clientY),
                original = g.scene.widgets.find((a) => a.id === g.id)!;
              const q = (v: number) => (snap ? Math.round(v / 20) * 20 : Math.round(v));
              const dx = p.x - g.point.x, dy = p.y - g.point.y;
              g.next = g.moveIds
                ? moveWidgets(g.scene, g.moveIds, dx ? q(original.x + dx) - original.x : 0, dy ? q(original.y + dy) - original.y : 0)
                : withWidget(g.scene, g.id, { width: q(original.width + dx), height: q(original.height + dy) });
              dispatch({ type: 'preview', scene: g.next });
            }}
            onPointerUp={(e) => {
              const g = gesture.current;
              if (g?.pointerId === e.pointerId) {
                gesture.current = null;
                dispatch({ type: 'commit', scene: g.next, before: g.scene });
              }
              panning.current = null;
              setIsPanning(false);
            }}
            onPointerCancel={(e) => {
              if (gesture.current?.pointerId === e.pointerId) {
                dispatch({ type: 'preview', scene: gesture.current.scene });
                gesture.current = null;
              }
              panning.current = null;
              setIsPanning(false);
            }}
          >
            <div
              ref={canvas}
              className="canvas"
              style={{
                width: 2240,
                height: 1080,
                '--s': scale,
                transform: `translate(-50%,-50%) translate(${pan.x}px,${pan.y}px) scale(${scale})`,
                backgroundColor: canvasBg,
                backgroundImage: showCanvasBg && scene.bgType !== 'solid' && previewBackground ? `url(${previewBackground})` : undefined,
                backgroundSize: '100% 100%',
                backgroundRepeat: 'no-repeat',
              } as CSSProperties}
              onPointerDown={(e) => {
                if (e.target === e.currentTarget) select(null);
              }}
            >
              {grid && <div className="grid-overlay" />}
              {scene.widgets
                .filter((a) => a.visible)
                .map((v) => (
                  <div
                    key={v.id}
                    className={`widget-box ${selected.has(v.id) ? 'selected' : ''} ${v.locked ? 'locked' : ''}`}
                    style={{ left: v.x, top: v.y, width: v.width, height: v.height }}
                    onPointerDown={(e) => {
                      if (e.button !== 0 || e.altKey || gesture.current) return;
                      e.stopPropagation();
                      if (e.ctrlKey || e.metaKey) {
                        select(v.id, true);
                        return;
                      }
                      const moveIds = selected.has(v.id) ? selected : new Set([v.id]);
                      if (!selected.has(v.id)) setSelected(moveIds);
                      if (v.locked) return;
                      gesture.current = { scene, next: scene, id: v.id, point: point(e.clientX, e.clientY), pointerId: e.pointerId, moveIds };
                      viewport.current!.setPointerCapture(e.pointerId);
                    }}
                  >
                    <Preview w={previewScene.widgets.find(a => a.id === v.id)!} readings={readings} cpuTempStatus={cpuTempStatus} />
                    {w?.id === v.id && !v.locked && (
                      <button
                        aria-label="Resize selected widget"
                        className="resize"
                        style={{ width: 14 / scale, height: 14 / scale }}
                        onPointerDown={(e) => {
                          if (e.button !== 0 || e.altKey || e.ctrlKey || e.metaKey || gesture.current) return;
                          e.stopPropagation();
                          gesture.current = { scene, next: scene, id: v.id, point: point(e.clientX, e.clientY), pointerId: e.pointerId };
                          viewport.current!.setPointerCapture(e.pointerId);
                        }}
                      />
                    )}
                  </div>
                ))}
              {bend && <div className="bend" style={{ left: profile.bend[0], width: profile.bend[1] - profile.bend[0] }} />}
              {division && <div className="division" style={{ left: profile.division }} />}
            </div>
          </div>

          <footer className="statusbar">
            <span className={`status ${tone}`} role="status" title={status}><span className="dot" /><span>{status}</span></span>
            <span className="coords">{w ? `${w.x}, ${w.y} · ${w.width} × ${w.height}` : selection.length > 1 ? `${selection.length} selected` : ''}</span>
            <span className="keys">Scroll to zoom · drag empty space to pan</span>
          </footer>
        </section>

        <aside className="sidebar right">
          {w ? (
            <>
              <div className="inspector-head">
                <span className="kind-badge"><Icon name={w.kind === 'Text' ? 'text' : sensorIcon(w.binding)} /></span>
                <input aria-label="Widget name" maxLength={120} value={w.name} {...live('name')} />
                <button className={`ghost icon-btn ${w.locked ? 'on' : ''}`} aria-label={w.locked ? 'Unlock widget' : 'Lock widget'} title={w.locked ? 'Unlock' : 'Lock'} onClick={() => patch({ locked: !w.locked })}>
                  <Icon name={w.locked ? 'lock' : 'unlock'} />
                </button>
              </div>
              <div className="sidebar-scroll">
                {w.kind === 'Metric value' && (
                  <section className="section">
                    <h3 className="section-title">Sensor</h3>
                    <label className="field">
                      Shows
                      <select value={w.binding}
                        onChange={(e) => patch({ binding: e.target.value, text: e.target.value.toUpperCase(), value: Math.min(w.value, metricMax(e.target.value)) })}>
                        {sensors.map(s => <option key={s.binding} value={s.binding}>{s.name} ({formatMetric(s.binding, null).unit})</option>)}
                      </select>
                    </label>
                    {w.binding === 'CPU temperature' ? (
                      <p className={`note ${readings.cpu_temp == null && cpuTempStatus ? 'warn' : ''}`}>
                        {readings.cpu_temp != null ? `Live reading: ${readings.cpu_temp} °C.` : cpuTempStatus ?? 'Waiting for the CPU temperature sensor…'}
                      </p>
                    ) : (
                      <NumberField label={`Value when no reading${isNetwork(w.binding) ? ' (bytes/s)' : ''}`} value={w.value} min={0} max={metricMax(w.binding)}
                        step={isNetwork(w.binding) ? 'any' : 1} {...numberProps('value')} />
                    )}
                  </section>
                )}

                <section className="section">
                  <h3 className="section-title">Text</h3>
                  <label className="field">
                    {w.kind === 'Text' ? 'Text' : 'Title'}
                    <input maxLength={500} value={w.text} {...live('text')} />
                  </label>
                  <label className="field">
                    Font
                    <select value={w.fontFamily || 'roboto'} onChange={(e) => patch({ fontFamily: e.target.value })}>
                      {fontFamilies.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
                    </select>
                  </label>
                  <div className="style-row">
                    <button className="toggle" aria-pressed={fam === 'roboto-black' || w.bold} disabled={singleWeight || fam === 'roboto-black'} onClick={() => patch({ bold: !w.bold })} title="Bold"><b>B</b> Bold</button>
                    <button className="toggle" aria-pressed={!singleWeight && w.italic} disabled={singleWeight} onClick={() => patch({ italic: !w.italic })} title="Italic"><i>I</i> Italic</button>
                  </div>
                  {singleWeight && <p className="note">The display only has the regular style of this font.</p>}
                  <div className="fields">
                    <NumberField label="Size" value={w.fontSize} min={12} max={200} {...numberProps('fontSize')} />
                    <label className="field">
                      Color
                      <span className="color-field">
                        <input type="color" value={w.color} aria-label="Text color" {...live('color')} />
                        <code>{w.color}</code>
                      </span>
                    </label>
                  </div>
                </section>

                <section className="section">
                  <h3 className="section-title">Position &amp; size</h3>
                  <div className="fields">
                    <NumberField label="X" value={w.x} min={0} max={2240 - w.width} disabled={w.locked} {...numberProps('x')} />
                    <NumberField label="Y" value={w.y} min={0} max={1080 - w.height} disabled={w.locked} {...numberProps('y')} />
                    <NumberField label="Width" value={w.width} min={40} max={2240} disabled={w.locked} {...numberProps('width')} />
                    <NumberField label="Height" value={w.height} min={40} max={1080} disabled={w.locked} {...numberProps('height')} />
                  </div>
                </section>

                <section className="section">
                  <h3 className="section-title">Arrange</h3>
                  <div className="actions">
                    <button onClick={() => order(1)}><Icon name="up" size={14} /> Forward</button>
                    <button onClick={() => order(-1)}><Icon name="down" size={14} /> Backward</button>
                    <button onClick={duplicate} title="Ctrl+D"><Icon name="copy" size={14} /> Duplicate</button>
                    <button onClick={() => patch({ visible: !w.visible })}><Icon name={w.visible ? 'eyeOff' : 'eye'} size={14} /> {w.visible ? 'Hide' : 'Show'}</button>
                    <button className="danger" disabled={w.locked} onClick={remove} title="Delete"><Icon name="trash" size={14} /> Delete</button>
                  </div>
                </section>
              </div>
            </>
          ) : selection.length > 1 ? (
            <div className="sidebar-scroll">
              <section className="section">
                <h3 className="section-title">{selection.length} widgets selected</h3>
                <p className="hint">Drag or use arrow keys to move unlocked widgets together.</p>
                <div className="actions">
                  <button onClick={duplicate} title="Ctrl+D"><Icon name="copy" size={14} /> Duplicate</button>
                  <button onClick={() => setSelectionFlag('visible', !selection.every(v => v.visible))}>
                    {selection.every(v => v.visible) ? 'Hide all' : 'Show all'}
                  </button>
                  <button onClick={() => setSelectionFlag('locked', !selection.every(v => v.locked))}>
                    {selection.every(v => v.locked) ? 'Unlock all' : 'Lock all'}
                  </button>
                  <button className="danger" disabled={selection.every(v => v.locked)} onClick={remove} title="Delete unlocked widgets">
                    <Icon name="trash" size={14} /> Delete unlocked
                  </button>
                </div>
              </section>
            </div>
          ) : (
            <div className="empty-state">
              <span className="kind-badge"><Icon name="pointer" size={20} /></span>
              <p>Select a widget on the display to edit it.</p>
              <div className="shortcuts">
                <kbd>Ctrl click</kbd><span>Add or remove selection</span>
                <kbd>Arrows</kbd><span>Nudge (Shift ×10)</span>
                <kbd>Ctrl D</kbd><span>Duplicate</span>
                <kbd>Delete</kbd><span>Remove</span>
                <kbd>Ctrl Z / Y</kbd><span>Undo / redo</span>
                <kbd>Ctrl S</kbd><span>Save</span>
                <kbd>Alt drag</kbd><span>Pan over widgets</span>
              </div>
            </div>
          )}
        </aside>
      </div>

      {(starting || youtubeBusy) && (
        <div className="busy-overlay" role="alert" aria-live="assertive">
          <div><span className="spinner large" /><p>{starting ? 'Opening your last layout…' : status}</p></div>
        </div>
      )}
    </div>
  );
}

if ((import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV === false) {
  document.addEventListener('contextmenu', (e) => e.preventDefault());
}

createRoot(document.getElementById('root')!).render(<App />);
