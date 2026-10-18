// Synthetic connect/preflight fixtures only. No real WebSocket connections.
const assert = require("node:assert/strict");
const test = require("node:test");
const vm = require("node:vm");
const { appSource } = require("./frontend-test-helpers.cjs");
const { fixture, deferred, plain } = require("./replay-workspace-fixture.cjs");
const settled = () => new Promise(resolve => setImmediate(resolve));

function setup() {
  const f = fixture(); f.a = f.tab("a"); f.b = f.tab("b"); f.seed([f.a, f.b]);
  for (const [tab, name] of [[f.a, "a"], [f.b, "b"]]) {
    Object.assign(tab, { type: "websocket", wsScheme: "wss", wsHost: `${name}.example.com`, wsPort: "443", wsPath: `/${name}`,
      wsStatus: "disconnected", wsHandshakeText: `X-Test-Identity: tab-${name}`, wsHandshakeEdited: true,
      wsHeaders: [], wsSetupQueue: [], wsFrames: [], wsLifecycleToken: 0 });
  }
  for (const [name, value] of Object.entries({ wsSchemeSelect: "wss", wsHostInput: "a.example.com",
    wsPortInput: "443", wsPathInput: "/a", wsHandshakeHeaders: "X-Test-Identity: tab-a" })) f.els[name] = { value };
  f.preflights = []; f.connects = []; f.disconnects = []; f.effects = []; f.errors = []; f.restores = [];
  Object.assign(f.context, {
    flushWorkspaceStateForReplayAction: () => { const d = deferred(); f.preflights.push(d); return d.promise; },
    flushWorkspaceState: async () => {},
    setWsReplayTargetInputValidity() {},
    cloneReplayTabState: tab => plain(tab),
    scheduleWorkspaceStateSave: () => f.effects.push("save"),
    renderWsStatus: () => f.effects.push("status"), renderWsSetupQueue: () => f.effects.push("setup"),
    renderWsFrameList: () => f.effects.push("frames"), resetWsReplayFrameTracking: () => f.effects.push("reset frames"),
    startWsPoll: () => f.effects.push("poll"), stopWsPoll() {}, clearWsFrameListRender() {},
    refreshWsReplayFramesOnce: async () => {}, refreshWsReplayFramesUntilSettled: async () => {},
    runSetupQueue: async () => {},
    disconnectWsReplayBackend: async (...args) => { f.disconnects.push(args); return { ok: true }; },
    handleWorkspaceActionError: error => f.errors.push(error),
    handleReplayWorkspaceRevisionConflict: snapshot => f.errors.push(snapshot),
    restoreReplayTabConflictState: (tab, snapshot) => { f.restores.push(snapshot); Object.assign(tab, snapshot); },
    fetch: (url, options) => {
      assert.equal(url, "/api/replay/ws-connect");
      const d = { url, options, ...deferred() }; f.connects.push(d); return d.promise;
    },
  });
  for (const name of ["wsConnect", "wsDisconnect", "cleanupWsReplayTab", "parseWsHandshakeHeaders", "isWsReplayTabAlive",
    "validateWsReplayTargetInput", "normalizeWsReplayTargetFields", "defaultWsPortForScheme", "explicitAuthorityPort", "isDefaultPortForScheme", "joinAuthority", "wsReplayHostHeaderValue",
    "readWorkspaceRevisionConflictPayload", "isWorkspaceRevisionConflictPayload"]) {
    const source = appSource.match(new RegExp(`^(?:async )?function ${name}\\([^]*?^\\}$`, "m"));
    assert.ok(source, name); vm.runInContext(source[0], f.context);
  }
  f.resolvePreflight = async (index = 0) => { f.preflights[index].resolve(10); await settled(); };
  f.finish = async running => { f.connects.at(-1).resolve({ ok: true }); await running; };
  f.interrupt = async kind => {
    if (kind === "close") await f.context.closeRepeaterTab(f.a.id);
    else if (kind === "disconnect") {
      const running = f.context.wsDisconnect(); await settled();
      await f.resolvePreflight(1); await running;
    } else if (kind === "replacement") f.state.replayTabs = [plain(f.a), f.b];
    else f.state.activeSession = { id: "session-b" };
  };
  f.snapshot = () => plain({ state: f.state, effects: f.effects, errors: f.errors, restores: f.restores, toasts: f.toasts });
  return f;
}

test("connect snapshots target and headers before switching to another tab during workspace save", async () => {
  const f = setup(), running = f.context.wsConnect();
  f.state.activeReplayTabId = f.b.id;
  f.els.wsHandshakeHeaders.value = "X-Test-Identity: tab-b";
  f.els.wsHostInput.value = "b.example.com"; f.els.wsPathInput.value = "/b";
  await f.resolvePreflight(); assert.equal(f.connects.length, 1);
  const payload = JSON.parse(f.connects[0].options.body);
  assert.deepEqual(payload, { session_id: "session-a", expected_active_session_id: "session-a", expected_workspace_revision: 10,
    id: f.a.id, scheme: "wss", host: "a.example.com", port: 443, path: "/a", headers: [{ name: "X-Test-Identity", value: "tab-a" }] });
  await f.finish(running);
});

test("connect cannot mix later model edits into its captured target or headers", async () => {
  const f = setup(), running = f.context.wsConnect();
  f.a.wsScheme = "ws"; f.a.wsHost = "edited.example.com"; f.a.wsHandshakeEdited = false;
  f.a.wsHeaders = [{ name: "X-Later", value: "ignored" }];
  f.els.wsHandshakeHeaders.value = "X-Test-Identity: later";
  await f.resolvePreflight();
  const payload = JSON.parse(f.connects[0].options.body);
  assert.equal(payload.scheme, "wss"); assert.equal(payload.host, "a.example.com");
  assert.deepEqual(payload.headers, [{ name: "X-Test-Identity", value: "tab-a" }]);
  await f.finish(running);
});

test("repeated connect clicks while preflight is pending do not supersede the operation", async () => {
  const f = setup(), running = f.context.wsConnect(), token = f.a.wsLifecycleToken;
  const repeated = f.context.wsConnect(); await settled();
  assert.equal(f.preflights.length, 1); assert.equal(f.a.wsLifecycleToken, token);
  await repeated; await f.resolvePreflight(); await f.finish(running);
});

test("an already connected tab does not initiate another connection", async () => {
  const f = setup(); f.a.wsStatus = "connected";
  const running = f.context.wsConnect(); await settled();
  assert.equal(f.preflights.length, 0); assert.equal(f.connects.length, 0); await running;
});

test("invalid handshake headers fail before workspace preflight or connection dispatch", async () => {
  const f = setup(); f.els.wsHandshakeHeaders.value = "invalid header";
  const running = f.context.wsConnect(); await settled();
  assert.equal(f.preflights.length, 0); assert.equal(f.connects.length, 0); await running;
  assert.equal(f.a.wsStatus, "error"); assert.match(f.a.wsError, /Invalid WebSocket handshake header/);
});

for (const kind of ["close", "disconnect", "session", "replacement"]) {
  for (const result of ["success", "conflict"]) {
    test(`connect preflight ${result} after ${kind} cannot dispatch or restore old state`, async () => {
      const f = setup(), running = f.context.wsConnect(); await f.interrupt(kind);
      const before = f.snapshot();
      if (result === "success") f.preflights[0].resolve(10);
      else f.preflights[0].reject(vm.runInContext("new WorkspaceStateConflictError({ revision: 11 })", f.context));
      await settled(); assert.equal(f.connects.length, 0); await running;
      assert.deepEqual(f.snapshot(), before);
    });
  }
}

test("disconnect and reconnect during old preflight only dispatches the newest connection", async () => {
  const f = setup(), old = f.context.wsConnect(); await f.interrupt("disconnect");
  f.els.wsHandshakeHeaders.value = "X-Test-Identity: reconnect";
  const newer = f.context.wsConnect();
  await f.resolvePreflight(); assert.equal(f.connects.length, 0); await old;
  await f.resolvePreflight(2); assert.equal(f.connects.length, 1);
  assert.equal(JSON.parse(f.connects[0].options.body).headers[0].value, "reconnect");
  await f.finish(newer);
});

for (const kind of ["close", "disconnect", "session", "replacement"]) {
  test(`delayed connect conflict after ${kind} cannot restore the stale tab`, async () => {
    const f = setup(), running = f.context.wsConnect(); await f.resolvePreflight();
    const body = deferred();
    f.connects[0].resolve({ ok: false, status: 409, headers: { get: () => "application/json" }, clone: () => ({ json: () => body.promise }) });
    await settled(); await f.interrupt(kind);
    const before = f.snapshot();
    body.resolve({ revision: 11, replay: {}, fuzzer: {} }); await running;
    assert.deepEqual(f.snapshot(), before);
  });
}
