import { metricMax } from './metrics.ts';
export const kinds = ['Metric value', 'Text'] as const;
export type Kind = typeof kinds[number];
export const fontFamilies = [
  { id: 'roboto', name: 'Roboto', css: "'Roboto', sans-serif" },
  { id: 'roboto-black', name: 'Roboto Black (Heavy Bold)', css: "'Roboto', sans-serif" },
  { id: 'orbitron', name: 'Orbitron', css: "'Orbitron', sans-serif" },
  { id: 'bitcount-single', name: 'Bitcount Single', css: "'Bitcount Single', monospace" },
  { id: 'rubik-glitch', name: 'Rubik Glitch', css: "'Rubik Glitch', cursive" },
] as const;

export function getCssFontFamily(family?: string): string {
  const match = fontFamilies.find((f) => f.id === family);
  return match ? match.css : "'Roboto', sans-serif";
}

export type Widget = {
  id: string;
  kind: Kind;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  visible: boolean;
  locked: boolean;
  text: string;
  color: string;
  background: string;
  fontSize: number;
  value: number;
  binding: string;
  image: string;
  fontFamily: string;
  bold: boolean;
  italic: boolean;
};
export type Scene = {
  version: 2;
  name: string;
  width: 2240;
  height: 1080;
  widgets: Widget[];
  grid: boolean;
  snap: boolean;
  split: boolean;
  bend: boolean;
  bgType?: 'preset' | 'solid';
  bgColor?: string;
  bgFile?: string;
};
export const profile = { division: 1120, bend: [1060, 1180] };
export const emptyScene = (): Scene => ({
  version: 2,
  name: 'Untitled layout',
  width: 2240,
  height: 1080,
  widgets: [],
  grid: true,
  snap: false,
  split: true,
  bend: true,
  bgType: 'preset',
  bgColor: '#000000',
});
export function geometry(w: Widget): Widget {
  const width = Math.max(40, Math.min(2240, Math.round(w.width))),
    height = Math.max(40, Math.min(1080, Math.round(w.height)));
  return {
    ...w,
    width,
    height,
    x: Math.max(0, Math.min(2240 - width, Math.round(w.x))),
    y: Math.max(0, Math.min(1080 - height, Math.round(w.y))),
  };
}
export function createWidget(kind: Kind, x = 100, y = 100): Widget {
  return geometry({
    id: crypto.randomUUID(),
    kind,
    name: kind === 'Text' ? 'Label Text' : 'Metric Value',
    x,
    y,
    width: kind === 'Text' ? 600 : 440,
    height: kind === 'Text' ? 120 : 220,
    visible: true,
    locked: false,
    text: kind === 'Text' ? 'SYSTEM OVERVIEW' : 'CPU TEMP',
    color: '#ffffff',
    background: '#18262e',
    fontSize: kind === 'Text' ? 42 : 64,
    value: 42,
    binding: 'CPU temperature',
    image: '',
    fontFamily: 'roboto',
    bold: false,
    italic: false,
  });
}
export function sample(): Scene {
  const title = createWidget('Text', 100, 70);
  title.name = 'Header';
  title.text = 'PANORAMA SE MONITOR';
  title.width = 800;
  title.height = 100;
  title.fontSize = 42;
  title.color = '#ffffff';

  const cpuLoad = createWidget('Metric value', 100, 200);
  cpuLoad.name = 'CPU Load';
  cpuLoad.text = 'CPU LOAD';
  cpuLoad.binding = 'CPU load';
  cpuLoad.width = 420;
  cpuLoad.height = 220;
  cpuLoad.value = 15;
  cpuLoad.color = '#ffffff';

  const cpuTemp = createWidget('Metric value', 560, 200);
  cpuTemp.name = 'CPU Temperature';
  cpuTemp.text = 'CPU TEMP';
  cpuTemp.binding = 'CPU temperature';
  cpuTemp.width = 420;
  cpuTemp.height = 220;
  cpuTemp.value = 42;
  cpuTemp.color = '#ffffff';

  const gpuTemp = createWidget('Metric value', 1240, 200);
  gpuTemp.name = 'GPU Temperature';
  gpuTemp.text = 'GPU TEMP';
  gpuTemp.binding = 'GPU temperature';
  gpuTemp.width = 420;
  gpuTemp.height = 220;
  gpuTemp.value = 46;
  gpuTemp.color = '#ffffff';

  return { ...emptyScene(), name: 'System Overview', widgets: [title, cpuLoad, cpuTemp, gpuTemp] };
}
export function logical(x: number, y: number, r: { left: number; top: number; width: number; height: number }) {
  return { x: ((x - r.left) * 2240) / r.width, y: ((y - r.top) * 1080) / r.height };
}
export function parseScene(text: string): Scene {
  if (text.length > 15_000_000) throw Error('Layout exceeds 15 MB');
  const s = JSON.parse(text.replace(/^\uFEFF/, ''));
  if (
    s?.version !== 2 ||
    s.width !== 2240 ||
    s.height !== 1080 ||
    typeof s.name !== 'string' ||
    s.name.length > 120 ||
    !Array.isArray(s.widgets) ||
    s.widgets.length > 100
  )
    throw Error('Unsupported layout');
  if (typeof s.grid !== 'boolean') s.grid = true;
  if (typeof s.snap !== 'boolean') s.snap = false;
  if (typeof s.split !== 'boolean') s.split = typeof (s as any).division === 'boolean' ? (s as any).division : true;
  if (typeof s.bend !== 'boolean') s.bend = true;
  if (s.bgType !== 'solid') s.bgType = 'preset';
  if (typeof s.bgColor !== 'string' || !/^#[0-9a-fA-F]{6}$/.test(s.bgColor)) s.bgColor = '#000000';
  else s.bgColor = s.bgColor.toLowerCase();
  if (s.bgFile !== undefined && (typeof s.bgFile !== 'string' || s.bgFile.length > 255 || !s.bgFile.endsWith('.mp4.h264_2240x1080') || /[\\/\x00-\x1f]/.test(s.bgFile))) {
    throw Error('Invalid background media file');
  }
  const ids = new Set();
  for (const w of s.widgets) {
    if (!w || !kinds.includes(w.kind) || typeof w.id !== 'string' || ids.has(w.id)) throw Error('Invalid widget ID/type');
    ids.add(w.id);
    if (typeof w.fontFamily !== 'string') w.fontFamily = 'roboto';
    if (typeof w.bold !== 'boolean') w.bold = false;
    if (typeof w.italic !== 'boolean') w.italic = false;
    for (const k of ['x', 'y', 'width', 'height', 'fontSize', 'value']) if (!Number.isFinite(w[k])) throw Error('Invalid number');
    for (const k of ['name', 'text', 'binding', 'image', 'color', 'background', 'fontFamily']) if (typeof w[k] !== 'string') throw Error('Invalid text');
    if (
      w.text.length > 500 ||
      w.name.length > 120 ||
      w.fontFamily.length > 50 ||
      w.fontSize < 12 ||
      w.fontSize > 200 ||
      w.value < 0 ||
      w.value > metricMax(w.binding) ||
      typeof w.visible !== 'boolean' ||
      typeof w.locked !== 'boolean' ||
      typeof w.bold !== 'boolean' ||
      typeof w.italic !== 'boolean' ||
      !/^#[0-9a-f]{6}$/i.test(w.color) ||
      !/^#[0-9a-f]{6}$/i.test(w.background)
    )
      throw Error('Invalid settings');
    const g = geometry(w);
    for (const k of ['x', 'y', 'width', 'height'] as const) if (g[k] !== w[k]) throw Error('Geometry outside canvas');
  }
  return s;
}
