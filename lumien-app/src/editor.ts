import type { Scene } from './scene';
import { sensors } from './metrics.ts';

export type History = { scene: Scene; past: Scene[]; future: Scene[] };
type Action = { type: 'commit'; scene: Scene; before?: Scene } | { type: 'preview' | 'replace'; scene: Scene } | { type: 'undo' | 'redo' };
export const initialHistory = (scene: Scene): History => ({ scene, past: [], future: [] });
export function historyReducer(state: History, action: Action): History {
  if (action.type === 'replace') return initialHistory(action.scene);
  if (action.type === 'preview') return { ...state, scene: action.scene };
  if (action.type === 'commit') {
    const before = action.before ?? state.scene;
    if (JSON.stringify(before) === JSON.stringify(action.scene)) return state;
    return { scene: action.scene, past: [...state.past.slice(-79), before], future: [] };
  }
  if (action.type === 'undo' && state.past.length) return {
    scene: state.past[state.past.length - 1], past: state.past.slice(0, -1), future: [state.scene, ...state.future],
  };
  if (action.type === 'redo' && state.future.length) return {
    scene: state.future[0], past: [...state.past, state.scene], future: state.future.slice(1),
  };
  return state;
}

// Live readings are preview/apply data; the saved fallback values stay in the document.
export function withTelemetry(scene: Scene, readings: Record<string, number | null>): Scene {
  return { ...scene, widgets: scene.widgets.map(w => {
    // CPU temperature is read directly in the preview and by the native LCD worker.
    if (w.binding === 'CPU temperature') return w;
    const key = sensors.find(s => s.binding === w.binding)?.key;
    const value = key ? readings[key] : null;
    return w.kind === 'Metric value' && typeof value === 'number' && Number.isFinite(value) ? { ...w, value } : w;
  }) };
}

export function takeStartupScene(pending: { current: Scene | null }, current: Scene, ready: boolean): Scene | null {
  const restored = pending.current;
  if (restored && JSON.stringify(restored) !== JSON.stringify(current)) pending.current = null;
  if (!ready || !pending.current) return null;
  pending.current = null;
  return restored;
}
