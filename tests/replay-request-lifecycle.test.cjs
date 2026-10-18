// Synthetic HTTP Replay operations using the existing workspace/tab fixture and
// real request parsing, close, cancellation and history code. No target traffic.
const assert = require("node:assert/strict");
const nodeTest = require("node:test");
const test = (name, run) => nodeTest(name, { timeout: 10000 }, run);
const vm = require("node:vm");
const { appSource } = require("./frontend-test-helpers.cjs");
const { fixture, deferred, plain } = require("./replay-workspace-fixture.cjs");

const settled = () => new Promise(resolve => setImmediate(resolve));
function setup() {
  const f = fixture();
  f.a = f.tab("a"); f.b = f.tab("b"); f.seed([f.a, f.b]);
  f.a.responseRecord = { id: "redirect", status: 302,
    response: { headers: [{ name: "location", value: "/next" }] } };
  f.preflights = []; f.replays = []; f.effects = []; f.errors = []; f.restores = [];
  Object.assign(f.context, {
    AbortController, REPEATER_HISTORY_LIMIT: 100,
    flushWorkspaceStateForReplayAction: () => {
      const pending = deferred(); f.preflights.push(pending); return pending.promise;
    },
    // Tab close still runs the production handler, but persistence is not the
    // subject of this fixture (covered by replay-workspace-*.test.cjs).
    flushWorkspaceState: async () => {},
    setReplayTargetInputValidity() {},
    handleWorkspaceActionError: error => f.errors.push(error),
    handleReplayWorkspaceRevisionConflict: snapshot => f.errors.push(snapshot),
    restoreReplayTabConflictState: (tab, snapshot) => {
      f.restores.push(snapshot); Object.assign(tab, snapshot);
    },
    scheduleWorkspaceStateSave: () => f.effects.push(["save"]),
    renderReplayRequestHighlight() {},
    renderReplayResponseOnly: tab => f.effects.push(["response", tab.id]),
    renderReplayEmptyResponse: tab => f.effects.push(["empty", tab.id]),
    renderReplayResponseView: text => f.effects.push(["pane", f.state.activeReplayTabId, text]),
    renderReplayViewTabs: () => f.effects.push(["views", f.state.activeReplayTabId]),
    syncReplayToolbar: tab => f.effects.push(["toolbar", tab.id, f.context.isReplayTabSending(tab.id)]),
    scheduleRefresh: () => f.effects.push(["refresh"]),
    fetch: (url, options) => {
      assert.equal(url, "/api/replay/send");
      const pending = { url, options, ...deferred() }; f.replays.push(pending); return pending.promise;
    },
  });
  f.els.replayResponseMeta = { textContent: "" };
  for (const name of [
    "sendReplay", "followRedirect", "isReplayTabStillCurrent", "setReplaySending", "cancelReplaySend", "clearReplaySendInFlight",
    "parseEditableRawRequest", "splitRawHttpMessage", "parseReplayHttpVersionToken", "headerValue",
    "editableRequestBodyLength", "validateRawHttpBodyFraming", "httpRequestAuthoritiesEquivalent", "normalizeHttpRequestAuthority",
    "replayEffectiveHttpVersion", "buildUrlFromTarget", "joinAuthority", "isDefaultPortForScheme",
    "redirectTargetsSameOrigin", "isRedirectCredentialHeader", "isRedirectRequestBodyHeader",
    "readReplaySendError", "isWorkspaceRevisionConflictPayload", "replaySentDraftUnchanged", "applyReplaySentDraftIfUnchanged",
    "targetStatesEquivalent", "replayTargetOverridePayload", "recordRepeaterHistory", "deriveRepeaterRequest",
  ]) {
    vm.runInContext(appSource.match(new RegExp(`^(?:async )?function ${name}\\([^]*?^\\}`, "m"))[0], f.context);
  }
  f.resolvePreflight = async (index = 0) => { f.preflights[index].resolve(10); await settled(); };
  f.decode = async (kind = "success", index = f.replays.length - 1) => {
    const body = deferred();
    f.replays[index].resolve({ ok: kind === "success", status: kind === "conflict" ? 409 : 502,
      headers: { get: () => "application/json" }, json: () => body.promise });
    await settled();
    return body;
  };
  f.finish = async (pending, id = "result", index = f.replays.length - 1) => {
    f.replays[index].resolve({ ok: true, json: async () => ({ id }) }); await pending;
  };
  f.interrupt = async kind => {
    if (kind === "close") await f.context.closeRepeaterTab(f.a.id);
    else if (kind === "cancel") f.context.cancelReplaySend();
    else if (kind === "replacement") f.state.replayTabs = [plain(f.a), f.b];
    else {
      if (kind === "session reset") f.context.clearReplaySendInFlight();
      f.state.activeSession = { id: "session-b" };
    }
  };
  f.snapshot = () => plain({ state: f.state, effects: f.effects, errors: f.errors, restores: f.restores, toasts: f.toasts });
  return f;
}

for (const host of ["127.0.0.1", "[2001:db8::7]", "example.com"]) {
  for (const scheme of ["http", "https"]) {
    const defaultPort = scheme === "https" ? "443" : "80";
    for (const port of ["", defaultPort, "8443"]) {
      for (const relative of [false, true]) {
        const authority = host + (port && port !== defaultPort ? `:${port}` : "");
        const sourceAuthority = host + (port ? `:${port}` : "");
        const url = `${scheme}://${sourceAuthority}`;
        test(`followRedirect preserves ${url} authority for ${relative ? "relative" : "absolute"} locations`, async () => {
          const f = setup();
          if (relative) {
            Object.assign(f.a, {
              baseRequest: { ...f.a.baseRequest, scheme, host: sourceAuthority },
              requestText: `GET /a HTTP/1.1\r\nHost: ${sourceAuthority}\r\n\r\n`,
              targetScheme: scheme, targetHost: host.replace(/^\[|\]$/g, ""), targetPort: port || defaultPort,
            });
          }
          f.a.responseRecord.response.headers[0].value = `${relative ? "" : url}/next?key=value#fragment`;
          f.syncDom();
          const pending = f.context.followRedirect();
          // Preflight also persists this base request, so it must already agree
          // with the target before either workspace validation or dispatch.
          assert.equal(f.a.baseRequest.host, authority);
          await f.resolvePreflight();
          const sent = JSON.parse(f.replays[0].options.body);
          assert.equal(sent.request.host, authority);
          assert.equal(sent.request.scheme, scheme);
          assert.equal(sent.request.path, "/next?key=value");
          assert.equal(sent.request.headers.find(header => header.name === "host").value, authority);
          assert.equal(sent.target, null, "redirect destination is the request authority, not an override");
          await f.finish(pending);
          assert.equal(f.a.targetManuallyEdited, false);
          assert.equal(f.a.historyEntries[0].request.host, authority);
          assert.equal(f.context.deriveRepeaterRequest(f.a).host, authority);
          assert.equal(f.a.historyEntries[0].targetPort, port || defaultPort);
        });
      }
    }
  }
}

for (const action of ["sendReplay", "followRedirect"]) {
  test(`${action} owns the tab before workspace preflight and blocks repeated actions`, async () => {
    const f = setup(), pending = f.context[action]();
    assert.equal(f.context.isReplayTabSending(f.a.id), true);
    await f.context.sendReplay(); await f.context.followRedirect();
    assert.equal(f.preflights.length, 1);
    await f.resolvePreflight();
    assert.equal(f.replays.length, 1);
    const sent = JSON.parse(f.replays[0].options.body);
    assert.equal(sent.session_id, "session-a"); assert.equal(sent.expected_active_session_id, "session-a");
    assert.equal(sent.expected_workspace_revision, 10);
    assert.equal(sent.request.path, action === "followRedirect" ? "/next" : "/a");
    await f.finish(pending);
    assert.equal(f.context.isReplayTabSending(f.a.id), false);
  });

  for (const change of ["close", "cancel", "session reset", "session identity", "replacement"]) {
    for (const outcome of ["success", "failure"]) {
      test(`${action} ignores preflight ${outcome} after ${change}`, async () => {
        const f = setup(), pending = f.context[action]();
        await f.interrupt(change);
        const before = f.snapshot();
        if (outcome === "success") f.preflights[0].resolve(10);
        else f.preflights[0].reject(vm.runInContext("new WorkspaceStateConflictError({ revision: 11 })", f.context));
        await settled();
        assert.equal(f.replays.length, 0);
        await pending;
        assert.deepEqual(plain(f.state), before.state);
        assert.deepEqual(f.errors, before.errors); assert.deepEqual(f.restores, before.restores);
        assert.equal(f.context._replaySendControllers.size, 0);
      });
    }
  }

  test(`${action} cancelled preflight cannot clear a newer send's owner`, async () => {
    const f = setup(), old = f.context[action]();
    f.context.cancelReplaySend();
    const newer = f.context.sendReplay(), controller = f.context._replaySendControllers.get(f.a.id);
    f.preflights[0].resolve(10); await settled();
    assert.equal(f.replays.length, 0);
    await old;
    assert.equal(f.context._replaySendControllers.get(f.a.id), controller);
    await f.resolvePreflight(1); await f.finish(newer, "newer");
    assert.equal(f.a.responseRecord.id, "newer");
  });

  test(`${action} preflight keeps another tab's response pane intact`, async () => {
    const f = setup(), pending = f.context[action]();
    f.state.activeReplayTabId = f.b.id; f.syncDom(); f.effects.length = 0;
    await f.resolvePreflight();
    assert.equal(f.effects.some(effect => effect[0] === "pane"), false);
    await f.finish(pending);
    assert.equal(f.a.responseRecord.id, "result"); assert.equal(f.b.responseRecord, null);
    assert.equal(f.effects.some(effect => effect[0] === "response"), false);
  });

  for (const kind of ["success", "error", "conflict"]) {
    const payload = kind === "success" ? { id: "old-result" }
      : kind === "error" ? { error: "old error", record: { id: "old-error-record" } }
        : { revision: 11, replay: { tabs: [] }, fuzzer: {} };
    test(`${action} retains ownership while decoding ${kind} and ignores repeated clicks`, async () => {
      const f = setup(), pending = f.context[action](); await f.resolvePreflight();
      const body = await f.decode(kind);
      assert.equal(f.context.isReplayTabSending(f.a.id), true);
      await f.context.sendReplay(); await f.context.followRedirect();
      assert.equal(f.preflights.length, 1); assert.equal(f.replays.length, 1);
      body.resolve(payload); await pending;
      assert.equal(f.context.isReplayTabSending(f.a.id), false);
      if (kind !== "conflict") assert.equal(f.a.historyEntries.length, 1);
      else assert.equal(f.restores.length, 1);
    });

    for (const change of ["close", "cancel", "session reset", "session identity", "replacement"]) {
      test(`${action} ignores decoded ${kind} after ${change}`, async () => {
        const f = setup(), pending = f.context[action](); await f.resolvePreflight();
        const body = await f.decode(kind); await f.interrupt(change);
        const before = f.snapshot();
        body.resolve(payload); await pending;
        assert.deepEqual(plain(f.state), before.state);
        assert.deepEqual(f.errors, before.errors); assert.deepEqual(f.restores, before.restores);
        assert.deepEqual(f.toasts, before.toasts);
        assert.equal(f.context._replaySendControllers.size, 0);
      });
    }

    test(`${action} cancelled ${kind} cannot overwrite a newer result or its history`, async () => {
      const f = setup(), old = f.context[action](); await f.resolvePreflight();
      const body = await f.decode(kind); f.context.cancelReplaySend();
      assert.equal(f.replays[0].options.signal.aborted, true);
      const newer = f.context.sendReplay(); await f.resolvePreflight(1); await f.finish(newer, "newer");
      const before = f.snapshot(); body.resolve(payload); await old;
      assert.deepEqual(f.snapshot(), before);
      assert.equal(f.a.responseRecord.id, "newer"); assert.equal(f.a.historyEntries.length, 1);
    });
  }

  for (const phase of ["fetch", "body"]) {
    for (const outcome of ["resolve", "reject"]) {
      test(`${action} old ${phase} ${outcome} cannot release a newer in-flight owner`, async () => {
        const f = setup(), old = f.context[action](); await f.resolvePreflight();
        const response = phase === "body" ? await f.decode() : f.replays[0];
        f.context.cancelReplaySend();
        const newer = f.context.sendReplay(); await f.resolvePreflight(1);
        const controller = f.context._replaySendControllers.get(f.a.id), before = f.snapshot();
        if (outcome === "reject") response.reject(new Error("late failure"));
        else response.resolve(phase === "body" ? { id: "old" } : { ok: true, json: async () => ({ id: "old" }) });
        await old;
        assert.deepEqual(f.snapshot(), before);
        assert.equal(f.context._replaySendControllers.get(f.a.id), controller);
        await f.finish(newer, "newer");
      });
    }
  }

  test(`${action} body decoding failure is handled and releases ownership`, async () => {
    const f = setup(), pending = f.context[action](); await f.resolvePreflight();
    const body = await f.decode(); body.reject(new SyntaxError("invalid response")); await pending;
    assert.equal(f.context.isReplayTabSending(f.a.id), false);
    assert.equal(f.a.notice, "invalid response");
    assert.equal(f.a.historyEntries.length, 1); assert.equal(f.a.historyEntries[0].responseRecord, null);
  });

  test(`${action} preserves a request edited while the body loads and records its response in history`, async () => {
    const f = setup(), pending = f.context[action](); await f.resolvePreflight();
    const body = await f.decode();
    f.a.requestText = "GET /edited HTTP/1.1\r\nHost: example.com\r\n\r\n";
    body.resolve({ id: "original" }); await pending;
    assert.match(f.a.requestText, /\/edited/); assert.equal(f.a.responseRecord, null);
    assert.equal(f.a.historyEntries[0].responseRecord.id, "original");
    assert.equal(f.context.isReplayTabSending(f.a.id), false);
  });
}
