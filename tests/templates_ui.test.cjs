const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const templates = require("../web/templates.js");

test("list queries page by the page size and drop empty filters", () => {
  assert.equal(
    templates.listQuery({ search: "  wordpress ", severity: "high", support: "passive", enabledOnly: true }, 2),
    "/api/templates?search=wordpress&severity=high&support=passive&enabled=true&limit=100&offset=200",
  );
  assert.equal(templates.listQuery({ search: " ", severity: "", support: "" }, -1), "/api/templates?limit=100&offset=0");
});

test("bulk changes only ever select passive-capable templates", () => {
  assert.deepEqual(templates.passiveRequest({ search: "waf", severity: "", enabledOnly: true }, true), {
    filter: { support: "passive", search: "waf" },
    enabled: true,
  });
  assert.deepEqual(templates.passiveRequest({}, 0), { filter: { support: "passive" }, enabled: false });
});

test("the library line says where templates came from and whether they were verified", () => {
  assert.match(templates.libraryLine(null), /Loading/);
  assert.match(templates.libraryLine({ library: null }), /No templates installed/);
  const line = templates.libraryLine({
    library: { source: "github", tag: "v10.5.0", verified: true, commit: "f8b8b8dcd70d1234567890abcdef12345678901" },
    templates: 14035, passive_supported: 2082, passive_enabled: 3,
  });
  assert.match(line, /nuclei-templates v10\.5\.0 · checksum verified · commit f8b8b8dcd70d/);
  assert.match(line, /2,082 can run passively, 3 on/);
  assert.match(templates.libraryLine({ library: { source: "import", verified: false }, templates: 1, passive_supported: 0, passive_enabled: 0 }), /Imported archive · not checksum-verified/);
});

test("support labels name the passive kind or the first reason it cannot run", () => {
  assert.equal(templates.supportLabel({ passive_supported: true, passive: "global" }), "Every response");
  assert.equal(templates.supportLabel({ passive_supported: true, passive: "base_url_get" }), "Captured GET");
  assert.equal(templates.supportLabel({ passive_supported: false, reasons: ["protocol:dns", "x"] }), "No: protocol:dns");
  assert.equal(templates.supportLabel({ passive_supported: true, passive: "base_url_get", active_supported: true }), "Captured GET · Scan");
  assert.equal(templates.supportLabel({ active_supported: true, reasons: ["randomness"] }), "Scan");
});

test("a scan request names its targets, its selection and whether it only counts", () => {
  const values = { targets: " https://app.example.com\nhttp://api.example.com:8080/v1 ", selection: "filter",
    risks: ["intrusive", "made-up"], rate: "5", concurrency: "2", maxRequests: "500", applyMatchReplace: false };
  assert.deepEqual(templates.scanRequest(values, { search: " wordpress ", severity: "high" }, null, "s-1", true), {
    session_id: "s-1",
    targets: ["https://app.example.com", "http://api.example.com:8080/v1"],
    allow_risks: ["intrusive"],
    rate: 5, concurrency: 2, max_requests: 500, apply_match_replace: false,
    filter: { search: "wordpress", severity: "high" },
    preview: true,
  });
  const one = templates.scanRequest({ ...values, selection: "selected" }, {}, "git-config", "s-1", false);
  assert.deepEqual(one.ids, ["git-config"]);
  assert.equal(one.acknowledged, true);
  assert.equal(one.filter, undefined);
  assert.throws(() => templates.scanRequest({ ...values, selection: "selected" }, {}, null, "s-1", true), /Select a template/);
  assert.throws(() => templates.scanRequest(values, {}, null, null, true), /session/);
  assert.throws(() => templates.scanRequest({ ...values, rate: "51" }, {}, null, "s-1", true), /from 1 to 50/);
  for (const bad of ["", "app.example.com", "https://app.example.com/?q=1", "https://user:pw@app.example.com", "ftp://app.example.com"]) {
    assert.throws(() => templates.parseTargets(bad), Error, bad);
  }
  assert.throws(() => templates.parseTargets(Array.from({ length: 21 }, (_, i) => `https://h${i}.example.com`).join(" ")), /at most 20/);
});

test("previews and jobs read as what was or will be sent", () => {
  assert.equal(
    templates.previewLine({ requests: 1200, templates: 300, targets: ["https://app.example.com"], max_requests: 10000, excluded_by_risk: { intrusive: 4 } }),
    "Send 1,200 requests from 300 templates to https://app.example.com, at most 10,000 counting redirects. Left out by risk: intrusive 4.",
  );
  assert.equal(
    templates.jobLine({ status: "running", sent: 10, planned: 20, matched: 1, failed: 2, blocked: 0 }),
    "running · 10 of 20 sent · 1 matched · 2 failed",
  );
});

test("template text is never written as markup", () => {
  const source = fs.readFileSync(path.join(__dirname, "../web/templates.js"), "utf8");
  assert.doesNotMatch(source, /innerHTML|insertAdjacentHTML|outerHTML/);
  assert.doesNotMatch(source, /\.href\s*=/);
});

test("the Templates tool is registered where the UI and the server keep their tool lists", () => {
  const root = path.join(__dirname, "..");
  const html = fs.readFileSync(path.join(root, "web/index.html"), "utf8");
  const app = fs.readFileSync(path.join(root, "web/app.js"), "utf8");
  const settings = fs.readFileSync(path.join(root, "src/ui_settings.rs"), "utf8");
  assert.match(html, /<button class="main-tab" data-tool="templates">Templates<\/button>/);
  assert.match(html, /<script src="\/templates\.js" defer><\/script>/);
  assert.match(app, /const IMPLEMENTED_TOOLS = new Set\(\[[^\]]*"templates"/);
  assert.match(settings, /const ACTIVE_TOOL_OPTIONS: &\[&str\] = &\[[^\]]*"templates"/);
  for (const id of ["Status", "Progress", "Consent", "Download", "Check", "Import", "ImportFile", "Notice", "ScopeNote",
    "Search", "Severity", "Support", "EnabledOnly", "EnableShown", "DisableShown", "Meta", "Rows", "Detail",
    "PageMeta", "Previous", "Next", "ScanTargets", "ScanSelection", "ScanRisks", "ScanRate", "ScanConcurrency",
    "ScanMaxRequests", "ScanMatchReplace", "ScanPreview", "ScanConsent", "ScanConsentText", "ScanStart", "ScanNotice",
    "ScanJobs"]) {
    assert.match(html, new RegExp(`id="templates${id}"`), id);
  }
  // Each risk the server allows has its box, and dos has none.
  const boxes = [...html.matchAll(/data-risk="([^"]+)"/g)].map((match) => match[1]);
  assert.deepEqual(boxes, templates.RISKS);
  assert.ok(!boxes.includes("dos"));
  assert.match(app, /getSessionId: currentSessionId,\s*openRecord: jumpToTransaction/);
  assert.equal((app.match(/templatesController\?\.syncSession\(\)/g) || []).length, 2);
});
