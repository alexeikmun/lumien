import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sample, parseScene, logical, geometry, createWidget, emptyScene } from './scene.ts';
import { RealDeviceAdapter } from './adapter.ts';
import { historyReducer, initialHistory, withTelemetry, takeStartupScene } from './editor.ts';

test('native coordinates invert zoom and pan', () => {
  for (const scale of [0.2, 0.5, 1, 2]) {
    const r = { left: 230, top: 170, width: 2240 * scale, height: 1080 * scale };
    assert.deepEqual(logical(230 + 1260 * scale, 170 + 80 * scale, r), { x: 1260, y: 80 });
  }
});

test('layout roundtrip, bounds and trust boundary', () => {
  const s = sample();
  assert.deepEqual(parseScene(JSON.stringify(s)), s);
  assert.equal(geometry({ ...createWidget('Text'), x: 9999 }).x, 1640);
  assert.throws(() => parseScene(JSON.stringify({ ...s, widgets: [s.widgets[0], s.widgets[0]] })));
  assert.throws(() => parseScene(JSON.stringify({ ...s, widgets: [{ ...s.widgets[0], x: NaN }] })));
});

test('sample layout is 100% hardware supported', () => {
  const s = sample();
  for (const w of s.widgets) {
    assert.ok(w.kind === 'Metric value' || w.kind === 'Text');
    assert.ok(w.x + w.width <= 2240);
    assert.ok(w.y + w.height <= 1080);
  }
});

test('real device adapter validates scene in browser mode without errors', async () => {
  const adapter = new RealDeviceAdapter();
  const s = sample();
  const res = await adapter.applyScene(s);
  assert.ok(res.includes('Preview: validated'));
});

test('live readings do not dirty document or undo history', () => {
  const scene = sample();
  const saved = JSON.stringify(scene);
  const preview = withTelemetry(scene, { cpu_load: 99, gpu_temp: 51 });
  assert.equal(JSON.stringify(scene), saved);
  assert.equal(preview.widgets.find(w => w.binding === 'CPU load')!.value, 99);
  assert.equal(initialHistory(scene).past.length, 0);
  assert.deepEqual(parseScene('\uFEFF' + saved), scene);
});

test('rename/drag form one undo step; branch clears redo; open starts fresh history', () => {
  const original = sample();
  const renamed = { ...original, name: 'Renamed layout' };
  let history = initialHistory(original);
  history = historyReducer(history, { type: 'preview', scene: { ...original, name: 'Ren' } });
  history = historyReducer(history, { type: 'preview', scene: renamed });
  history = historyReducer(history, { type: 'commit', scene: renamed, before: original });
  assert.equal(history.past.length, 1);
  history = historyReducer(history, { type: 'undo' });
  assert.deepEqual(history.scene, original);
  history = historyReducer(history, { type: 'redo' });
  assert.deepEqual(history.scene, renamed);
  history = historyReducer(history, { type: 'undo' });
  history = historyReducer(history, { type: 'commit', scene: { ...original, name: 'Different' } });
  assert.equal(history.future.length, 0);
  history = historyReducer(history, { type: 'replace', scene: emptyScene() });
  assert.equal(history.past.length, 0);
  assert.equal(history.future.length, 0);
});

test('startup apply waits for USB and consumes restored scene exactly once', () => {
  const scene = sample();
  const pending = { current: scene as typeof scene | null };
  assert.equal(takeStartupScene(pending, scene, false), null);
  assert.equal(pending.current, scene);
  assert.equal(takeStartupScene(pending, scene, true), scene);
  assert.equal(takeStartupScene(pending, scene, true), null);
  assert.equal(takeStartupScene(pending, scene, false), null);
  // A later reconnect cannot replay a consumed/failed write.
  assert.equal(takeStartupScene(pending, scene, true), null);
});

test('editing while waiting cancels stale startup apply; missing restored file never applies sample', () => {
  const scene = sample();
  const pending = { current: scene as typeof scene | null };
  assert.equal(takeStartupScene(pending, { ...scene, name: 'Edited' }, false), null);
  assert.equal(pending.current, null);
  assert.equal(takeStartupScene(pending, scene, true), null);
  assert.equal(takeStartupScene({ current: null }, scene, true), null);
});
