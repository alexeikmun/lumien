import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sample, parseScene, logical, geometry, createWidget, emptyScene } from './scene.ts';
import { RealDeviceAdapter } from './adapter.ts';
import { historyReducer, initialHistory, withTelemetry, takeStartupScene } from './editor.ts';
import { formatMetric, sensors } from './metrics.ts';

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

test('network byte rates format and persist above percentage limits', () => {
  const scene = emptyScene();
  scene.widgets = [{ ...createWidget('Metric value'), binding: 'Network download', value: 1_250_000.5 }];
  assert.deepEqual(parseScene(JSON.stringify(scene)), scene);
  const preview = withTelemetry(scene, { network_download: 2_500_000 });
  assert.equal(scene.widgets[0].value, 1_250_000.5);
  assert.deepEqual(formatMetric('Network download', preview.widgets[0].value), { text: '2.5', unit: 'MB/s' });
  assert.deepEqual(formatMetric('Network upload', 100_000), { text: '0.1', unit: 'MB/s' });
  assert.deepEqual(formatMetric('Network upload', 0), { text: '0', unit: 'MB/s' });
  assert.deepEqual(formatMetric('Network download', 2e9), { text: '2000', unit: 'MB/s' });
  assert.equal(formatMetric('Network download', null).text, '--');
  assert.equal(sensors.length, 7);
  assert.throws(() => parseScene(JSON.stringify({ ...scene, widgets: [{ ...scene.widgets[0], value: -1 }] })));
  assert.throws(() => parseScene(JSON.stringify({ ...scene, widgets: [{ ...scene.widgets[0], binding: 'CPU load' }] })));
});

test('font family and styles default gracefully and persist', () => {
  const scene = emptyScene();
  const w = createWidget('Text');
  w.fontFamily = 'orbitron';
  w.bold = true;
  w.italic = true;
  scene.widgets = [w];
  const roundtrip = parseScene(JSON.stringify(scene));
  assert.equal(roundtrip.widgets[0].fontFamily, 'orbitron');
  assert.equal(roundtrip.widgets[0].bold, true);
  assert.equal(roundtrip.widgets[0].italic, true);

  // Backward compatibility: JSON missing font fields defaults to roboto, false, false
  const legacyWidget = { ...w };
  delete (legacyWidget as any).fontFamily;
  delete (legacyWidget as any).bold;
  delete (legacyWidget as any).italic;
  const parsedLegacy = parseScene(JSON.stringify({ ...scene, widgets: [legacyWidget] }));
  assert.equal(parsedLegacy.widgets[0].fontFamily, 'roboto');
  assert.equal(parsedLegacy.widgets[0].bold, false);
  assert.equal(parsedLegacy.widgets[0].italic, false);
});

test('snap, grid, split, and bend toolbar options persist and default gracefully', () => {
  const scene = emptyScene();
  scene.snap = true;
  scene.grid = false;
  scene.split = false;
  scene.bend = false;
  const roundtrip = parseScene(JSON.stringify(scene));
  assert.equal(roundtrip.snap, true);
  assert.equal(roundtrip.grid, false);
  assert.equal(roundtrip.split, false);
  assert.equal(roundtrip.bend, false);

  // Backward compatibility: legacy scene without toolbar options defaults gracefully
  const legacyScene = { ...scene };
  delete (legacyScene as any).snap;
  delete (legacyScene as any).grid;
  delete (legacyScene as any).split;
  delete (legacyScene as any).bend;
  const parsedLegacy = parseScene(JSON.stringify(legacyScene));
  assert.equal(parsedLegacy.snap, false);
  assert.equal(parsedLegacy.grid, true);
  assert.equal(parsedLegacy.split, true);
  assert.equal(parsedLegacy.bend, true);
});
