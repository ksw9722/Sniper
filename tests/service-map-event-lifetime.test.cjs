// Synthetic event ownership only. No app bootstrap, sockets, or captured traffic.
const assert = require('node:assert/strict');
const test = require('node:test');
const { appSource, loadFunctions } = require('./frontend-test-helpers.cjs');

function fixture() {
  const streams = [], calls = [];
  let session = 'session-one';
  const liveStatus = { textContent: 'Proxy live', classList: { remove(value) { calls.push(['remove', value]); } } };
  const context = loadFunctions(['connectEvents'], {
    eventSource: null, workspaceStateGeneration: 1, targetServiceMapEventStreamReady: () => false,
    currentSessionId: () => session,
    sessionQueryPath: (path, id) => `${path}?session_id=${id}`,
    els: { liveStatus },
    targetServiceMapController: { disconnect() { calls.push(['disconnect']); } },
    EventSource: function (url) {
      this.url = url; this.readyState = 0; this.listeners = new Map();
      this.addEventListener = (type, callback) => this.listeners.set(type, callback);
      this.close = () => { this.closed = true; };
      streams.push(this);
    },
  });
  return { context, streams, calls, liveStatus, setSession(value) { session = value; } };
}

test('owned SSE error drops temporary map evidence without starting a read', () => {
  const f = fixture(); f.context.connectEvents();
  f.streams[0].onerror();
  assert.deepEqual(f.calls, [['disconnect'], ['remove', 'online']]);
  assert.equal(f.liveStatus.textContent, 'Retrying');
  assert.equal(f.streams.length, 1);
});

for (const boundary of ['replacement', 'session', 'workspace']) {
  test(`stale ${boundary} stream error cannot clear current map evidence or status`, () => {
    const f = fixture(); f.context.connectEvents(); const old = f.streams[0];
    if (boundary === 'replacement') f.context.connectEvents();
    if (boundary === 'session') f.setSession('session-two');
    if (boundary === 'workspace') f.context.workspaceStateGeneration += 1;
    old.onerror();
    assert.deepEqual(f.calls, []);
    assert.equal(f.liveStatus.textContent, 'Proxy live');
  });
}

test('event error still updates live status before map controller initialization', () => {
  const f = fixture(); delete f.context.targetServiceMapController;
  f.context.connectEvents(); f.streams[0].onerror();
  assert.deepEqual(f.calls, [['remove', 'online']]);
  assert.equal(f.liveStatus.textContent, 'Retrying');
});

test('temporary overlay status is wired into the production controller', () => {
  assert.match(appSource, /overlayStatus: document\.getElementById\("serviceMapBodyOverlay"\)/);
  assert.match(appSource, /getEventStreamReady: \(\) => targetServiceMapEventStreamReady\(\)/);
  assert.match(appSource, /setViewChange\(\(view\) => setActiveTargetView\(view, \{ refresh: false \}\)\)/);
});

for (const previous of ['service-map', 'apis']) {
  test(`internal ${previous} to map navigation uses the loaded snapshot without another request`, () => {
    const calls = [];
    const context = loadFunctions(['setActiveTargetView'], {
      state: { activeTool: 'map', activeTargetView: previous },
      document: { querySelectorAll: () => [], getElementById: () => null },
      currentSessionId: () => 'session-one',
      targetServiceMapController: {
        setVisible(...args) { calls.push(['visible', ...args]); }, refresh() { calls.push(['read']); },
      },
      loadTargetSiteMap() { throw new Error('Legacy paths must not load'); },
    });
    context.setActiveTargetView('service-map', { refresh: false });
    assert.deepEqual(calls, [['visible', true, 'service-map']]);
    assert.equal(context.state.activeTargetView, 'service-map');
    context.setActiveTargetView('service-map');
    assert.deepEqual(calls.at(-1), ['read']);
  });
}

test('map overlay readiness is false while connecting or disconnected, true only for the open stream', () => {
  const f = fixture();
  assert.equal(f.context.targetServiceMapEventStreamReady(), false);
  f.context.connectEvents(); const current = f.streams[0];
  assert.equal(f.context.targetServiceMapEventStreamReady(), false);
  current.readyState = 1;
  assert.equal(f.context.targetServiceMapEventStreamReady(), true);
  current.readyState = 0; current.onerror();
  assert.equal(f.context.targetServiceMapEventStreamReady(), false);
  current.readyState = 2;
  assert.equal(f.context.targetServiceMapEventStreamReady(), false);
});

for (const boundary of ['replacement', 'session', 'workspace']) {
  test(`overlay readiness rejects a stale ${boundary} stream even if it remains open`, () => {
    const f = fixture(); f.context.connectEvents(); const old = f.streams[0]; old.readyState = 1;
    const readiness = f.context.targetServiceMapEventStreamReady;
    assert.equal(readiness(), true);
    if (boundary === 'replacement') f.context.connectEvents();
    if (boundary === 'session') f.setSession('session-two');
    if (boundary === 'workspace') f.context.workspaceStateGeneration += 1;
    assert.equal(readiness(), false);
    assert.equal(f.context.targetServiceMapEventStreamReady(), false);
  });
}
