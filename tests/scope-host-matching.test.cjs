// The client's scope check must agree with src/scope.rs: History drops a row
// whose update the client calls out of scope.
const assert = require("node:assert/strict");
const test = require("node:test");
const { loadFunctions } = require("./frontend-test-helpers.cjs");

function inScope(host, scope_patterns = [], excluded_scope_patterns = []) {
  const state = { runtime: { scope_patterns, excluded_scope_patterns } };
  const context = loadFunctions(["isInScopeHost", "hostMatchesScopePattern", "normalizeHostForMatching", "hostWithoutPort"], { state });
  return context.isInScopeHost(host);
}

test("globs, root dots and ports match as the server's matcher does", () => {
  assert.equal(inScope("api-v2.example.com", ["api-*.example.com"]), true);
  assert.equal(inScope("web.example.com", ["api-*.example.com"]), false);
  assert.equal(inScope("anything.example.net", ["*"]), true);
  assert.equal(inScope("example.com.", ["*.example.com"]), true);
  assert.equal(inScope("a.example.com:8443", ["*.example.com"]), true);
  assert.equal(inScope("notexample.com", ["*.example.com"]), false);
  assert.equal(inScope("EXAMPLE.com", ["example.com"]), true);
  assert.equal(inScope("ab", ["a*b*b"]), false, "literals cannot overlap");
});

test("excludes work with and without includes", () => {
  assert.equal(inScope("log-1.example.com", [], ["log-*.example.com"]), false);
  assert.equal(inScope("app.example.com", [], ["log-*.example.com"]), true);
  assert.equal(inScope("log-1.example.com", ["*.example.com"], ["log-*.example.com"]), false);
  assert.equal(inScope("app.example.com", ["*.example.com"], ["log-*.example.com"]), true);
});
