import { useRef, useState, useEffect, useReducer } from 'react';
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
import { historyReducer, initialHistory, withTelemetry, takeStartupScene } from './editor';
import { sensors, isNetwork, metricMax, formatMetric } from './metrics';
import './style.css';

function Preview({ w, readings }: { w: Widget; readings: Record<string, number | null> }) {
  const key = sensors.find(s => s.binding === w.binding)?.key;
  const available = !isNetwork(w.binding) || (key && readings[key] != null);
  const value = w.binding === 'CPU temperature' ? readings.cpu_temp : available ? w.value : null;
  const metric = formatMetric(w.binding, value);
  const fontFam = getCssFontFamily(w.fontFamily);
  const fam = (w.fontFamily || 'roboto').toLowerCase();
  const isRoboto = fam === 'roboto' || fam === 'roboto-black';
  const isBlack = fam === 'roboto-black';
  const fontStyle = isRoboto && w.italic ? 'italic' : 'normal';
  const fontWeight = isBlack ? 900 : isRoboto && w.bold ? 700 : 400;

  return (
    <div
      className="widget"
      style={{
        background: w.background,
        color: w.color,
        fontSize: w.fontSize,
        fontFamily: fontFam,
        fontStyle,
      }}
    >
      {w.kind === 'Text' ? (
        <div style={{ fontWeight }}>{w.text}</div>
      ) : (
        <>
          <small style={{ fontWeight }}>{w.text}</small>
          <strong style={{ fontWeight }}>
            {metric.text}
            <em style={{ fontStyle: isRoboto && w.italic ? 'italic' : 'normal' }}>{metric.unit}</em>
          </strong>
        </>
      )}
    </div>
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

type DownloadedVideo = { id: string; title: string; duration: number; bytes: number; thumbnail: string };
type LayoutOption = { path: string; name: string };

function App() {
  const desktop = '__TAURI_INTERNALS__' in window;
  const [youtubeUrl, setYoutubeUrl] = useState('');
  const [youtubeBusy, setYoutubeBusy] = useState(false);
  const [downloadedVideos, setDownloadedVideos] = useState<DownloadedVideo[]>([]);
  const [galleryError, setGalleryError] = useState('');
  const [galleryLoading, setGalleryLoading] = useState(false);
  const [selectedVideo, setSelectedVideo] = useState<string | null>(null);
  const [layouts, setLayouts] = useState<LayoutOption[]>([]);
  const [{ scene, past, future }, dispatch] = useReducer(historyReducer, undefined, () => initialHistory(sample()));
  const [selected, select] = useState<string | null>(null),
    [saved, setSaved] = useState<string | null>(null),
    [filePath, setFilePath] = useState<string | null>(null),
    [fileBusy, setFileBusy] = useState(desktop),
    [starting, setStarting] = useState(desktop),
    [fileError, setFileError] = useState<string | null>(null),
    [readings, setReadings] = useState<Record<string, number | null>>({}),
    [pending, setPending] = useState<'new' | { scene: Scene; path: string } | null>(null),
    [zoom, setZoom] = useState(1),
    [fit, setFit] = useState(0.4),
    [pan, setPan] = useState({ x: 0, y: 0 }),
    [status, setStatus] = useState('Ready'),
    [busy, setBusy] = useState(false),
    [activeTab, setActiveTab] = useState<'widgets' | 'backgrounds'>('widgets'),
    [bgSubtab, setBgSubtab] = useState<'presets' | 'solid'>(() => {
      return (localStorage.getItem('lumien_bg_subtab') as 'presets' | 'solid') || 'presets';
    }),
    [solidColor, setSolidColor] = useState<string>(() => {
      return localStorage.getItem('lumien_solid_color') || '#000000';
    }),
    [hexInput, setHexInput] = useState<string>(() => {
      return (localStorage.getItem('lumien_solid_color') || '#000000').toUpperCase();
    }),
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

  useEffect(() => {
    if (!desktop || activeTab !== 'backgrounds' || youtubeBusy) return;
    let active = true;
    setGalleryLoading(true); setGalleryError('');
    invoke<DownloadedVideo[]>('list_downloaded_videos')
      .then(videos => { if (active) setDownloadedVideos(videos); })
      .catch(error => { if (active) setGalleryError(String(error)); })
      .finally(() => { if (active) setGalleryLoading(false); });
    return () => { active = false; };
  }, [desktop, activeTab, youtubeBusy]);

  useEffect(() => {
    if (desktop) invoke<LayoutOption[]>('list_layouts').then(setLayouts).catch(error => setStatus(String(error)));
  }, [desktop]);

  const renameStart = useRef<Scene | null>(null);
  const colorStart = useRef<Scene | null>(null);
  const discardDialog = useRef<HTMLDialogElement>(null);
  const fileOperation = useRef(false);
  const startupApply = useRef<Scene | null>(null);
  const viewport = useRef<HTMLDivElement>(null),
    canvas = useRef<HTMLDivElement>(null);
  const gesture = useRef<{ scene: Scene; id: string; point: { x: number; y: number }; resize: boolean } | null>(null),
    panning = useRef<{ x: number; y: number; px: number; py: number } | null>(null);

  const w = scene.widgets.find((v) => v.id === selected),
    scale = fit * zoom,
    dirty = !starting && saved !== JSON.stringify(scene),
    previewScene = withTelemetry(scene, readings),
    grid = scene.grid ?? true,
    snap = scene.snap ?? false,
    division = scene.split ?? true,
    bend = scene.bend ?? true;

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    fileOperation.current = true;
    invoke<{ path: string; content: string } | null>('restore_layout')
      .then(async (file) => {
        if (active && file) {
          const restored = await hydrateBackground(parseScene(file.content));
          if (!active) return;
          replaceDocument(restored, file.path, false);
          startupApply.current = restored;
          if (restored.bgType === 'solid') {
            setBgSubtab('solid');
            if (restored.bgColor) {
              setSolidColor(restored.bgColor);
              setHexInput(restored.bgColor.toUpperCase());
            }
          }
          setStatus('Reopened ' + file.path);
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
      .then((list) => {
        setPresets(list);
      })
      .catch((err) => {
        console.error('Failed to list presets:', err);
      })
      .finally(() => setPresetsLoading(false));
  }, [desktop]);

  useEffect(() => {
    if (scene.bgType === 'solid') {
      setBgSubtab('solid');
      setSolidColor(scene.bgColor ?? '#000000');
      setHexInput((scene.bgColor ?? '#000000').toUpperCase());
      return;
    }
    if (scene.bgFile) {
      const preset = presets.find((item) => item.file_name === scene.bgFile);
      setActivePresetFile(scene.bgFile);
      setPreviewBackground(preset?.thumbnail ?? '');
      setBgSubtab('presets');
      if (!preset && !presetsLoading) setStatus(`Background preview unavailable for saved media: ${scene.bgFile}. Apply will still send its saved filename to the LCD.`);
      return;
    }
    const active = presets.find((item) => item.is_active);
    if (active) {
      setActivePresetFile(active.file_name);
      setPreviewBackground(active.thumbnail);
    }
  }, [scene.bgType, scene.bgColor, scene.bgFile, presets, presetsLoading]);

  const importPresets = async () => {
    if (!desktop) return;
    setImporting(true);
    setStatus('Importing presets from KANALI...');
    try {
      const list = await invoke<PresetItem[]>('import_kanali_presets');
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

  const applyPreset = async (p: PresetItem) => {
    if (!desktop) return;
    setStatus(`Applying background "${p.name}" to display...`);
    try {
      await invoke('set_display_background', { fileName: p.file_name });
      setActivePresetFile(p.file_name);
      setPreviewBackground(p.thumbnail);
      setPresets((prev) =>
        prev.map((item) => ({ ...item, is_active: item.file_name === p.file_name }))
      );
      const updatedScene: Scene = { ...scene, bgType: 'preset', bgFile: p.file_name };
      commit(updatedScene);
      localStorage.setItem('lumien_bg_subtab', 'presets');
      setStatus(`Background "${p.name}" applied.`);
      if (scene.widgets.some((widget) => widget.visible)) {
        await device.applyScene(updatedScene);
      }
    } catch (err) {
      setStatus(`Failed to apply background: ${err}`);
    }
  };

  // The native color input fires on every drag tick: preview while picking, commit one undo step on blur.
  const onColorPickerChange = (color: string) => {
    const norm = normalizeHex(color);
    setSolidColor(norm);
    setHexInput(norm.toUpperCase());
    localStorage.setItem('lumien_solid_color', norm);
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
      localStorage.setItem('lumien_solid_color', norm);
      commit({ ...scene, bgType: 'solid', bgColor: norm, bgFile: undefined });
    }
  };

  const onSelectSwatch = (hex: string) => {
    const norm = normalizeHex(hex);
    setSolidColor(norm);
    setHexInput(norm.toUpperCase());
    localStorage.setItem('lumien_solid_color', norm);
    commit({ ...scene, bgType: 'solid', bgColor: norm, bgFile: undefined });
  };

  const applySolidColor = async (color: string) => {
    const norm = normalizeHex(color);
    setSolidColor(norm);
    setHexInput(norm.toUpperCase());
    localStorage.setItem('lumien_bg_subtab', 'solid');
    localStorage.setItem('lumien_solid_color', norm);
    const updatedScene: Scene = { ...scene, bgType: 'solid', bgColor: norm, bgFile: undefined };
    commit(updatedScene);
    setStatus(`Applying solid background (${norm}) to display...`);
    try {
      if (desktop) {
        await invoke('set_solid_color_background', { color: norm });
        setActivePresetFile('');
        setPreviewBackground('');
      }
      if (scene.widgets.some((widget) => widget.visible)) {
        await device.applyScene(updatedScene);
      }
      setStatus(`Solid color background (${norm}) applied to display.`);
    } catch (err) {
      setStatus(`Failed to apply solid color: ${err}`);
    }
  };

  useEffect(() => {
    const wasPending = startupApply.current !== null;
    const restored = takeStartupScene(startupApply, scene,
      desktop && !starting && !fileBusy && !pending && !busy && deviceState.connected);
    if (!restored) {
      if (wasPending && !startupApply.current) setStatus('Startup apply cancelled: layout changed. Use Apply to Display when ready.');
      else if (startupApply.current && !starting && !deviceState.connected) setStatus('Layout restored. Waiting for LCD connection…');
      return;
    }
    setBusy(true);
    setStatus('Applying restored layout to LCD…');
    void (async () => {
      try {
        if (desktop) {
          if (restored.bgType === 'solid') {
            await invoke('set_solid_color_background', { color: restored.bgColor });
          } else if (restored.bgFile) {
            await invoke('set_display_background', { fileName: restored.bgFile });
          }
        }
        const message = await device.applyScene(parseScene(JSON.stringify(withTelemetry(restored, readings))));
        setStatus('Startup apply: ' + message);
      } catch (e) {
        setStatus('Startup apply failed: ' + String(e) + '. Use Apply to Display to retry.');
      } finally { setBusy(false); }
    })();
  }, [desktop, starting, fileBusy, pending, busy, deviceState.connected, scene]);

  useEffect(() => {
    const r = new ResizeObserver(() => {
      if (viewport.current) {
        setFit(Math.min((viewport.current.clientWidth - 100) / 2240, (viewport.current.clientHeight - 100) / 1080));
      }
    });
    if (viewport.current) r.observe(viewport.current);
    return () => r.disconnect();
  }, []);

  // Poll device connection status
  useEffect(() => {
    let active = true;
    const check = async () => {
      const s = await device.getStatus();
      if (active) setDeviceState(s);
    };
    check();
    const id = setInterval(check, 3000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  // Poll live hardware telemetry
  useEffect(() => {
    let active = true;
    const poll = async () => {
      if (!desktop) return;
      try {
        const telemetry = await device.getTelemetry();
        if (!active || Object.keys(telemetry).length === 0) return;
        setReadings(telemetry);
      } catch {
        // silent telemetry poll catch
      }
    };
    const id = setInterval(poll, 1500);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [desktop]);

  function commit(next: Scene) {
    dispatch({ type: 'commit', scene: next });
  }

  function patch(p: Partial<Widget>) {
    if (w) commit({ ...scene, widgets: scene.widgets.map((v) => (v.id === w.id ? geometry({ ...v, ...p }) : v)) });
  }

  function undo() { dispatch({ type: 'undo' }); }
  function redo() { dispatch({ type: 'redo' }); }

  function duplicate() {
    if (!w) return;
    const copy = geometry({ ...w, id: crypto.randomUUID(), name: w.name + ' copy', x: w.x + 20, y: w.y + 20, locked: false });
    commit({ ...scene, widgets: [...scene.widgets, copy] });
    select(copy.id);
  }

  function remove() {
    if (w && !w.locked) {
      commit({ ...scene, widgets: scene.widgets.filter((v) => v.id !== w.id) });
      select(null);
    }
  }

  function add(kind: Kind, x = 120, y = 120) {
    if (scene.widgets.length >= 100) {
      setStatus('Editor limit: 100 widgets');
      return;
    }
    const a = createWidget(kind, x, y);
    commit({ ...scene, widgets: [...scene.widgets, a] });
    select(a.id);
  }

  function addSensor(binding: string, name: string, text: string, color = '#ffffff') {
    if (scene.widgets.length >= 100) {
      setStatus('Editor limit: 100 widgets');
      return;
    }
    const offset = (scene.widgets.length % 6) * 30;
    const a = geometry({
      id: crypto.randomUUID(),
      kind: 'Metric value' as const,
      name,
      x: 120 + offset,
      y: 120 + offset,
      width: isNetwork(binding) ? 600 : 420,
      height: 220,
      visible: true,
      locked: false,
      text,
      color,
      background: '#18262e',
      fontSize: 64,
      value: 0,
      binding,
      image: '',
      fontFamily: 'roboto',
      bold: false,
      italic: false,
    });
    commit({ ...scene, widgets: [...scene.widgets, a] });
    select(a.id);
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
    const activeFile = await invoke<string>('get_active_preset');
    const solid = /^solid_([0-9a-f]{6})\.mp4\.h264_2240x1080$/i.exec(activeFile);
    return solid
      ? { ...layout, bgType: 'solid', bgColor: `#${solid[1].toLowerCase()}` }
      : { ...layout, bgFile: activeFile };
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
      if (pending || fileBusy) return;
      if ((e.ctrlKey || e.metaKey) && ['s', 'n'].includes(e.key.toLowerCase())) {
        e.preventDefault();
        if (e.key.toLowerCase() === 's') void save(e.shiftKey);
        else newLayout();
        return;
      }
      if ((e.target as HTMLElement).closest('input,textarea,select,[contenteditable]')) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') {
        e.preventDefault();
        e.shiftKey ? redo() : undo();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'y') { e.preventDefault(); redo(); return; }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'd') {
        e.preventDefault();
        duplicate();
        return;
      }
      if (e.key === 'Delete') {
        remove();
        return;
      }
      if (!w || w.locked) return;
      const n = e.shiftKey ? 10 : 1;
      const d: Record<string, [number, number]> = {
        ArrowLeft: [-n, 0],
        ArrowRight: [n, 0],
        ArrowUp: [0, -n],
        ArrowDown: [0, n],
      };
      if (d[e.key]) {
        e.preventDefault();
        patch({ x: w.x + d[e.key][0], y: w.y + d[e.key][1] });
      }
    }
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });

  return (
    <div className="app" inert={starting || youtubeBusy} aria-busy={starting}>
      <header>
        <div className="brand">
          <span>◈</span> Lumien
        </div>
        <input
          aria-label="Layout name"
          className="layout-name"
          maxLength={120}
          value={scene.name}
          disabled={fileBusy || !!pending}
          title={filePath ?? 'Layout has not been saved to a file'}
          onFocus={() => { renameStart.current = scene; }}
          onChange={(e) => dispatch({ type: 'preview', scene: { ...scene, name: e.target.value } })}
          onBlur={() => {
            if (renameStart.current) dispatch({ type: 'commit', scene, before: renameStart.current });
            renameStart.current = null;
          }}
        />
        <span className="dirty" role="status" aria-label="Layout save status" title={filePath ?? 'No file yet'}>{dirty ? '● Unsaved' : filePath ? 'Saved' : 'New layout'}</span>
        <button disabled={fileBusy || !!pending} onClick={newLayout} title="New layout (Ctrl+N)">New</button>
        <select aria-label="Choose layout to preview" className="layout-picker" value={filePath ?? ''}
          disabled={fileBusy || !!pending} onChange={e => void selectLayout(e.target.value)}>
          <option value="">Choose layout…</option>
          {layouts.map(layout => <option key={layout.path} value={layout.path}>{layout.name}</option>)}
        </select>
        <button disabled={fileBusy || !!pending} onClick={() => void save()} title="Save (Ctrl+S); Save As (Ctrl+Shift+S)">{fileBusy ? 'Working…' : 'Save'}</button>
        <i />
        <button aria-label="Undo" disabled={!past.length || fileBusy || !!pending} onClick={undo} title="Undo (Ctrl+Z)">↶</button>
        <button aria-label="Redo" disabled={!future.length || fileBusy || !!pending} onClick={redo} title="Redo (Ctrl+Shift+Z / Ctrl+Y)">↷</button>
        <div className={`device-dot ${deviceState.connected ? 'connected' : ''}`}>
          {deviceState.connected ? '● Device Online' : '○ Device Disconnected'}
        </div>
        <button
          disabled={busy || !deviceState.connected}
          onClick={async () => {
            startupApply.current = null;
            setBusy(true);
            try {
              setStatus(await device.clearDisplay());
            } catch (e) {
              setStatus(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          Clear Screen
        </button>
        <button
          className="primary"
          disabled={busy}
          onClick={async () => {
            startupApply.current = null;
            setBusy(true);
            try {
              if (desktop) {
                if (previewScene.bgType === 'solid') {
                  await invoke('set_solid_color_background', { color: previewScene.bgColor });
                } else if (previewScene.bgFile) {
                  await invoke('set_display_background', { fileName: previewScene.bgFile });
                }
              }
              setStatus(await device.applyScene(parseScene(JSON.stringify(previewScene))));
            } catch (e) {
              setStatus(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy ? 'Applying…' : 'Apply to Display'}
        </button>
      </header>

      <dialog ref={discardDialog} className="document-dialog" aria-labelledby="unsaved-title" onCancel={(e) => { e.preventDefault(); if (!fileBusy) setPending(null); }}>
        <h2 id="unsaved-title">Save changes to {scene.name || 'Untitled layout'}?</h2>
        <p>Your layout has unsaved changes.</p>
        {fileError && <p role="alert">{fileError}</p>}
        <div>
          <button autoFocus disabled={fileBusy} onClick={() => setPending(null)}>Cancel</button>
          <button disabled={fileBusy} onClick={continuePending}>Discard changes</button>
          <button className="primary" disabled={fileBusy} onClick={async () => { if (await save()) continuePending(); }}>Save changes</button>
        </div>
      </dialog>
      <div className="body">
        <aside className="left">
          <div className="sidebar-tabs">
            <button
              className={`sidebar-tab ${activeTab === 'widgets' ? 'active' : ''}`}
              onClick={() => setActiveTab('widgets')}
            >
              Widgets
            </button>
            <button
              className={`sidebar-tab ${activeTab === 'backgrounds' ? 'active' : ''}`}
              onClick={() => setActiveTab('backgrounds')}
            >
              Backgrounds {presets.length > 0 && <span className="tab-badge">{presets.length}</span>}
            </button>
          </div>

          {activeTab === 'widgets' ? (
            <>
              <h2>INSERT WIDGET</h2>
              <p className="muted">Drag onto the display canvas</p>
              <div className="palette">
                {kinds.map((kind, i) => (
                  <button
                    key={kind}
                    draggable
                    onDragStart={(e) => e.dataTransfer.setData('lumien/widget', kind)}
                    onClick={() => add(kind)}
                  >
                    <span>{['42', 'T'][i]}</span>
                    {kind}
                  </button>
                ))}
              </div>
              <div className="section-heading">
                <h2>LAYERS</h2>
                <span>{scene.widgets.length}</span>
              </div>
              <div className="layers">
                {[...scene.widgets].reverse().map((v) => (
                  <div key={v.id} className={'layer ' + (selected === v.id ? 'active' : '')}>
                    <button className="layer-name" onClick={() => select(v.id)}>
                      {v.name}
                    </button>
                    <button
                      aria-label={'Toggle visibility ' + v.name}
                      onClick={() =>
                        commit({
                          ...scene,
                          widgets: scene.widgets.map((a) => (a.id === v.id ? { ...a, visible: !a.visible } : a)),
                        })
                      }
                    >
                      {v.visible ? '◉' : '○'}
                    </button>
                    <button
                      aria-label={'Toggle lock ' + v.name}
                      onClick={() =>
                        commit({
                          ...scene,
                          widgets: scene.widgets.map((a) => (a.id === v.id ? { ...a, locked: !a.locked } : a)),
                        })
                      }
                    >
                      {v.locked ? '◆' : '◇'}
                    </button>
                  </div>
                ))}
              </div>
              <div className="section-heading" style={{ marginTop: '20px' }}>
                <h2>AVAILABLE SENSORS</h2>
                <span>{sensors.length}</span>
              </div>
              <div className="sensor-list" style={{ display: 'flex', flexDirection: 'column', gap: '6px', margin: '8px 0 16px' }}>
                {sensors.map((s) => (
                  <button
                    key={s.binding}
                    style={{ textAlign: 'left', padding: '6px 10px', fontSize: '11px', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}
                    onClick={() => addSensor(s.binding, s.name, s.text)}
                  >
                    <span>+ {s.name}</span>
                    <small style={{ color: '#ffffff', fontWeight: 600 }}>{formatMetric(s.binding, null).unit}</small>
                  </button>
                ))}
              </div>
            </>
          ) : (
            <div className="presets-container">
              <section className="youtube-panel" aria-label="YouTube video">
                <h2>YOUTUBE VIDEO</h2>
                <label htmlFor="youtube-url">YouTube link</label>
                <input id="youtube-url" type="url" placeholder="https://www.youtube.com/watch?v=…" value={youtubeUrl} onChange={e => setYoutubeUrl(e.target.value)} />
                <p>Recorded videos up to 5 minutes. Silent playback, cropped to fill the LCD. Widgets are hidden during playback.</p>
                <button disabled={!desktop || !deviceState.connected || busy || !youtubeUrl.trim()} onClick={async () => {
                  setYoutubeBusy(true); setBusy(true);
                  setStatus('Downloading and converting YouTube video, then uploading to display…');
                  try { setStatus(await invoke<string>('play_youtube', { url: youtubeUrl.trim() })); }
                  catch (error) { setStatus('YouTube: ' + String(error)); }
                  finally { setYoutubeBusy(false); setBusy(false); }
                }}>Play full screen</button>
              </section>
              <section className="downloaded-gallery" aria-label="Downloaded videos">
                <h2>DOWNLOADED VIDEOS ({downloadedVideos.length})</h2>
                {galleryLoading && <p role="status">Loading videos…</p>}
                {galleryError && <p role="alert">{galleryError}</p>}
                {!galleryLoading && !galleryError && downloadedVideos.length === 0 && <p className="muted">Videos downloaded above appear here for replay.</p>}
                {downloadedVideos.map(video => (
                  <article key={video.id} className={`downloaded-video ${selectedVideo === video.id ? 'selected' : ''}`}>
                    <div className="downloaded-video-thumb">
                      {video.thumbnail ? <img src={video.thumbnail} alt={`${video.title} thumbnail`} /> : <span className="video-placeholder">▶</span>}
                      <button type="button" className="video-delete" disabled={busy}
                        aria-label={`Remove ${video.title} and duplicate downloads`} title="Remove downloaded video"
                        onClick={async () => {
                          if (!window.confirm(`Remove “${video.title}” and all duplicate downloads from this computer?`)) return;
                          setBusy(true);
                          try {
                            await invoke('remove_downloaded_video', { id: video.id });
                            setDownloadedVideos(items => items.filter(item => item.id !== video.id));
                            if (selectedVideo === video.id) setSelectedVideo(null);
                            setStatus('Downloaded video and duplicate copies removed.');
                          } catch (error) { setStatus('Could not remove video: ' + String(error)); }
                          finally { setBusy(false); }
                        }}>
                        <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M3 6h18M8 6V4h8v2m3 0-1 14H6L5 6m4 4v6m6-6v6" /></svg>
                      </button>
                    </div>
                    <button type="button" className="downloaded-video-play" disabled={busy || !deviceState.connected}
                      aria-pressed={selectedVideo === video.id} title={`Play ${video.title} on display`}
                      onClick={async () => {
                        setYoutubeBusy(true); setBusy(true); setStatus('Uploading saved video to display…');
                        try {
                          setStatus(await invoke<string>('play_downloaded_video', { id: video.id }));
                          setSelectedVideo(video.id);
                        } catch (error) { setStatus('Video playback failed: ' + String(error)); }
                        finally { setYoutubeBusy(false); setBusy(false); }
                      }}>
                      <strong>{video.title}</strong>
                      <small>{Math.floor(video.duration / 60)}:{String(Math.floor(video.duration % 60)).padStart(2, '0')} · {(video.bytes / 1048576).toFixed(1)} MB</small>
                      <span>{selectedVideo === video.id ? 'Selected · Play again' : '▶ Play on display'}</span>
                    </button>
                  </article>
                ))}
              </section>
              <div className="presets-header" style={{ marginBottom: '12px' }}>
                <h2>BACKGROUND OPTIONS</h2>
                <div className="bg-subtabs">
                  <button
                    className={`bg-subtab ${bgSubtab === 'presets' ? 'active' : ''}`}
                    onClick={() => {
                      setBgSubtab('presets');
                      localStorage.setItem('lumien_bg_subtab', 'presets');
                    }}
                  >
                    3D Presets ({presets.length})
                  </button>
                  <button
                    className={`bg-subtab ${bgSubtab === 'solid' ? 'active' : ''}`}
                    onClick={() => {
                      setBgSubtab('solid');
                      localStorage.setItem('lumien_bg_subtab', 'solid');
                      commit({ ...scene, bgType: 'solid', bgColor: solidColor, bgFile: undefined });
                    }}
                  >
                    Solid Color
                  </button>
                </div>
              </div>

              {bgSubtab === 'presets' ? (
                <>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px' }}>
                    <span style={{ fontSize: '11px', color: '#8fa0aa' }}>ANIMATION LOOPS</span>
                    <button
                      className="import-btn"
                      disabled={importing}
                      onClick={importPresets}
                      title="Scan and import all presets from KANALI into Lumien storage"
                    >
                      {importing ? 'Importing from KANALI...' : '↻ Import from KANALI'}
                    </button>
                  </div>

                  {presetsLoading ? (
                    <p className="muted">Loading presets...</p>
                  ) : presets.length === 0 ? (
                    <div style={{ textAlign: 'center', padding: '20px 0', color: '#92a2ad' }}>
                      <p>No presets found.</p>
                      <button className="import-btn" onClick={importPresets}>
                        Import from KANALI
                      </button>
                    </div>
                  ) : (
                    <div className="presets-list">
                      {presets.map((p) => {
                        const isActive = scene.bgType !== 'solid' && p.file_name === activePresetFile;
                        return (
                          <div
                            key={p.id}
                            className={`preset-card ${isActive ? 'active' : ''}`}
                            onClick={() => applyPreset(p)}
                            title={`Click to set background to ${p.name}`}
                          >
                            <div className="preset-thumb-wrapper">
                              {p.thumbnail ? (
                                <img className="preset-thumb" src={p.thumbnail} alt={p.name} />
                              ) : (
                                <div style={{ width: '100%', height: '100%', background: '#1c242b', display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#556570', fontSize: '11px' }}>
                                  No preview
                                </div>
                              )}
                              <span className={`preset-badge ${p.is_builtin ? 'builtin' : ''}`}>
                                {p.is_builtin ? 'Built-in' : '3D Preset'}
                              </span>
                            </div>
                            <div className="preset-info">
                              <span className="preset-title">{p.name}</span>
                              {isActive ? (
                                <span className="active-pill">
                                  <span className="active-dot" /> ACTIVE
                                </span>
                              ) : (
                                <button
                                  className="preset-apply-btn"
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    applyPreset(p);
                                  }}
                                >
                                  Apply
                                </button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </>
              ) : (
                <div className="solid-color-panel">
                  <div className="color-preview-card">
                    <div className="color-preview-box" style={{ backgroundColor: solidColor }}>
                      <span className="preset-badge" style={{ position: 'absolute', top: 6, left: 6 }}>
                        SOLID
                      </span>
                    </div>
                    <div className="color-preview-info">
                      <span className="color-hex-tag">{solidColor.toUpperCase()}</span>
                      {scene.bgType === 'solid' ? (
                        <span className="active-pill">
                          <span className="active-dot" /> ACTIVE
                        </span>
                      ) : (
                        <button
                          className="preset-apply-btn"
                          onClick={() => applySolidColor(solidColor)}
                        >
                          Apply
                        </button>
                      )}
                    </div>
                  </div>

                  <div>
                    <label style={{ fontSize: '11px', color: '#92a2ad', marginBottom: '6px', display: 'block' }}>
                      CUSTOM COLOR / HEX
                    </label>
                    <div className="color-controls">
                      <label className="color-picker-label">
                        <span>Choose any color</span>
                      <input
                        type="color"
                        aria-label="Choose background color"
                        className="color-picker-input"
                        value={solidColor}
                        onFocus={() => { colorStart.current = scene; }}
                        onChange={(e) => onColorPickerChange(e.target.value)}
                        onBlur={onColorPickerDone}
                        title="Open color picker"
                      />
                      </label>
                      <input
                        type="text"
                        className="color-hex-input"
                        aria-label="Background hex color"
                        value={hexInput}
                        onChange={(e) => onHexInputChange(e.target.value)}
                        placeholder="#000000"
                        maxLength={7}
                        spellCheck={false}
                      />
                    </div>
                  </div>

                  <div>
                    <span style={{ fontSize: '10px', letterSpacing: '1px', color: '#8fa0aa', fontWeight: 600, display: 'block', marginBottom: '8px' }}>
                      PRESELECTED PALETTE
                    </span>
                    <div className="swatches-grid">
                      {SOLID_SWATCHES.map((sw) => {
                        const isSelected = solidColor.toLowerCase() === sw.hex.toLowerCase();
                        return (
                          <button
                            key={sw.hex}
                            type="button"
                            className={`swatch-btn ${isSelected ? 'active' : ''}`}
                            aria-pressed={isSelected}
                            onClick={() => onSelectSwatch(sw.hex)}
                            title={`${sw.name} (${sw.hex}) — ${sw.note}`}
                          >
                            <span className="swatch-circle" style={{ backgroundColor: sw.hex }} />
                            <span className="swatch-name">{sw.name}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>

                  <button
                    className="solid-apply-btn"
                    onClick={() => applySolidColor(solidColor)}
                  >
                    Apply to Display
                  </button>
                </div>
              )}
            </div>
          )}

          <div className="capabilities">
            <strong>HARDWARE DISPLAY</strong>
            <p>2240 × 1080 AMOLED Curved</p>
            <p>Front: 1120×1080 · Bend: 1060–1180 · Side: 1120×1080</p>
            <p>{deviceState.message}</p>
          </div>
        </aside>

        <section className="center">
          <div className="canvas-toolbar">
            <span>
              DISPLAY <b>2240 × 1080</b>
            </span>
            <label>
              <input type="checkbox" checked={grid} onChange={(e) => commit({ ...scene, grid: e.target.checked })} />
              Grid
            </label>
            <label>
              <input type="checkbox" checked={snap} onChange={(e) => commit({ ...scene, snap: e.target.checked })} />
              Snap
            </label>
            <label>
              <input type="checkbox" checked={division} onChange={(e) => commit({ ...scene, split: e.target.checked })} />
              Split
            </label>
            <label>
              <input type="checkbox" checked={bend} onChange={(e) => commit({ ...scene, bend: e.target.checked })} />
              Bend
            </label>
            {(previewBackground || scene.bgType === 'solid') && (
              <label title="Show background in canvas">
                <input type="checkbox" checked={showCanvasBg} onChange={(e) => setShowCanvasBg(e.target.checked)} />
                BG
              </label>
            )}
            <button onClick={() => setZoom((z) => Math.max(0.25, z / 1.25))}>−</button>
            <span>{Math.round(scale * 100)}%</span>
            <button onClick={() => setZoom((z) => Math.min(5, z * 1.25))}>+</button>
            <button
              onClick={() => {
                setZoom(1);
                setPan({ x: 0, y: 0 });
              }}
            >
              Fit
            </button>
          </div>

          <div
            className="viewport"
            ref={viewport}
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
              if (!g) return;
              const p = point(e.clientX, e.clientY),
                original = g.scene.widgets.find((a) => a.id === g.id)!;
              const q = (v: number) => (snap ? Math.round(v / 20) * 20 : Math.round(v));
              const next = geometry({
                ...original,
                ...(g.resize
                  ? { width: q(original.width + p.x - g.point.x), height: q(original.height + p.y - g.point.y) }
                  : { x: q(original.x + p.x - g.point.x), y: q(original.y + p.y - g.point.y) }),
              });
              dispatch({ type: 'preview', scene: { ...g.scene, widgets: g.scene.widgets.map((a) => (a.id === g.id ? next : a)) } });
            }}
            onPointerUp={() => {
              if (gesture.current) {
                const before = gesture.current.scene;
                if (JSON.stringify(before) !== JSON.stringify(scene)) {
                  dispatch({ type: 'commit', scene, before });
                }
                gesture.current = null;
              }
              panning.current = null;
            }}
            onPointerCancel={() => {
              if (gesture.current) dispatch({ type: 'preview', scene: gesture.current.scene });
              gesture.current = null;
              panning.current = null;
            }}
          >
            <div
              ref={canvas}
              className={'canvas ' + (grid ? 'grid' : '')}
              style={{
                width: 2240,
                height: 1080,
                transform: `translate(-50%,-50%) translate(${pan.x}px,${pan.y}px) scale(${scale})`,
                backgroundColor: showCanvasBg
                  ? (scene.bgType === 'solid' ? solidColor : (previewBackground ? '#000000' : '#0c151d'))
                  : '#0c151d',
                backgroundImage: showCanvasBg && scene.bgType !== 'solid' && previewBackground
                  ? `url(${previewBackground})`
                  : undefined,
                backgroundSize: '100% 100%',
                backgroundRepeat: 'no-repeat',
              }}
              onPointerDown={(e) => {
                if (e.target === e.currentTarget) select(null);
              }}
            >
              {scene.widgets
                .filter((a) => a.visible)
                .map((v) => (
                  <div
                    key={v.id}
                    className={'widget-box ' + (selected === v.id ? 'selected' : '')}
                    style={{ left: v.x, top: v.y, width: v.width, height: v.height }}
                    onPointerDown={(e) => {
                      if (e.button !== 0 || e.altKey) return;
                      e.stopPropagation();
                      select(v.id);
                      if (v.locked) return;
                      gesture.current = { scene, id: v.id, point: point(e.clientX, e.clientY), resize: false };
                      viewport.current!.setPointerCapture(e.pointerId);
                    }}
                  >
                    <Preview w={previewScene.widgets.find(a => a.id === v.id)!} readings={readings} />
                    {selected === v.id && !v.locked && (
                      <button
                        aria-label="Resize selected widget"
                        className="resize"
                        style={{ width: 12 / scale, height: 12 / scale }}
                        onPointerDown={(e) => {
                          e.stopPropagation();
                          gesture.current = { scene, id: v.id, point: point(e.clientX, e.clientY), resize: true };
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

          <footer>
            <span>{w ? `X ${w.x} · Y ${w.y} / ${w.width} × ${w.height} px` : 'No selection'}</span>
            <span>Drag workspace to pan · Alt+drag over display</span>
          </footer>
          <div className="notice" role="status">
            {status}
          </div>
        </section>

        <aside className="right">
          <h2>INSPECTOR</h2>
          {w ? (
            <>
              <label>
                Name
                <input maxLength={120} value={w.name} onChange={(e) => patch({ name: e.target.value })} />
              </label>
              <h3>Transform</h3>
              <div className="fields">
                {(['x', 'y', 'width', 'height'] as const).map((k) => (
                  <label key={k}>
                    {k}
                    <input
                      disabled={w.locked}
                      type="number"
                      value={w[k]}
                      onChange={(e) => {
                        if (Number.isFinite(e.target.valueAsNumber)) patch({ [k]: e.target.valueAsNumber });
                      }}
                    />
                  </label>
                ))}
              </div>
              <label>
                <input type="checkbox" checked={w.visible} onChange={(e) => patch({ visible: e.target.checked })} />
                Visible
              </label>
              <label>
                <input type="checkbox" checked={w.locked} onChange={(e) => patch({ locked: e.target.checked })} />
                Locked
              </label>

              <h3>Appearance</h3>
              <label>
                Label Text
                <input maxLength={500} value={w.text} onChange={(e) => patch({ text: e.target.value })} />
              </label>
              <label>
                Font Family
                <select value={w.fontFamily || 'roboto'} onChange={(e) => patch({ fontFamily: e.target.value })}>
                  {fontFamilies.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.name}
                    </option>
                  ))}
                </select>
              </label>
              {(() => {
                const fam = (w.fontFamily || 'roboto').toLowerCase();
                const isNonRoboto = fam === 'orbitron' || fam === 'bitcount-single' || fam === 'rubik-glitch';
                const isBlack = fam === 'roboto-black';
                return (
                  <>
                    <label style={{ opacity: isNonRoboto || isBlack ? 0.6 : 1 }}>
                      <input
                        type="checkbox"
                        checked={isBlack ? true : Boolean(w.bold)}
                        disabled={isNonRoboto || isBlack}
                        onChange={(e) => patch({ bold: e.target.checked })}
                      />
                      Bold
                    </label>
                    {isNonRoboto && (
                      <small style={{ color: '#88a0b0', fontSize: '11px', display: 'block', marginTop: '-6px' }}>
                        Hardware SoC only includes Regular weight for this font.
                      </small>
                    )}
                    {isBlack && (
                      <small style={{ color: '#88a0b0', fontSize: '11px', display: 'block', marginTop: '-6px' }}>
                        Roboto Black is ultra-heavy bold (weight 900).
                      </small>
                    )}
                  </>
                );
              })()}
              <label>
                Font Size
                <input
                  type="number"
                  min={12}
                  max={200}
                  value={w.fontSize}
                  onChange={(e) => patch({ fontSize: Math.max(12, Math.min(200, Number(e.target.value) || 12)) })}
                />
              </label>
              <label>
                Accent Color
                <input type="color" value={w.color} onChange={(e) => patch({ color: e.target.value })} />
              </label>

              {w.kind === 'Metric value' && (
                <>
                  <h3>Hardware Telemetry Binding</h3>
                  <label>
                    Sensor Binding
                    <select
                      value={w.binding}
                      onChange={(e) =>
                        patch({
                          binding: e.target.value,
                          text: e.target.value.toUpperCase(),
                          value: Math.min(w.value, metricMax(e.target.value)),
                        })
                      }
                    >
                      {sensors.map(s => <option key={s.binding} value={s.binding}>{s.name} ({formatMetric(s.binding, null).unit})</option>)}
                    </select>
                  </label>
                  <label>
                    Fallback Value {isNetwork(w.binding) ? '(bytes/s)' : ''}
                    <input
                      type="number"
                      min={0}
                      max={metricMax(w.binding)}
                      step={isNetwork(w.binding) ? 'any' : 1}
                      value={w.value}
                      onChange={(e) => {
                        const value = Math.max(0, Math.min(metricMax(w.binding), Number(e.target.value) || 0));
                        patch({ value });
                      }}
                    />
                  </label>
                </>
              )}

              <h3>Layer Actions</h3>
              <div className="actions">
                <button onClick={() => order(1)}>Bring Forward</button>
                <button onClick={() => order(-1)}>Send Backward</button>
                <button onClick={duplicate}>Duplicate</button>
                <button disabled={w.locked} onClick={remove}>
                  Delete
                </button>
              </div>
            </>
          ) : (
            <div className="empty">
              <span>⌖</span>
              <p>Select a widget on the canvas to inspect and edit its properties</p>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}

if ((import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV === false) {
  document.addEventListener('contextmenu', (e) => e.preventDefault());
}

createRoot(document.getElementById('root')!).render(<App />);
