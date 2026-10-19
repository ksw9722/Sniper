(function (root) {
  "use strict";

  // Templates come from a public repository: every name, tag, description
  // and YAML line is rendered as text, never as markup, and references are
  // shown, never linked.
  const PAGE_SIZE = 100;
  const BUSY_STATES = new Set(["loading", "downloading", "importing"]);
  const PASSIVE_KINDS = { global: "Every response", base_url_get: "Captured GET" };

  function listQuery(filters, page) {
    const params = new URLSearchParams();
    const search = String(filters.search || "").trim();
    if (search) params.set("search", search);
    if (filters.severity) params.set("severity", filters.severity);
    if (filters.support) params.set("support", filters.support);
    if (filters.enabledOnly) params.set("enabled", "true");
    params.set("limit", String(PAGE_SIZE));
    params.set("offset", String(Math.max(0, page) * PAGE_SIZE));
    return `/api/templates?${params}`;
  }

  // Turning templates on in bulk follows the list filter, restricted to the
  // ones that can run passively; the server ignores the rest.
  function passiveRequest(filters, enabled) {
    const filter = { support: "passive" };
    const search = String(filters.search || "").trim();
    if (search) filter.search = search;
    if (filters.severity) filter.severity = filters.severity;
    return { filter, enabled: !!enabled };
  }

  function libraryLine(status) {
    if (!status) return "Loading the template library…";
    const library = status.library;
    if (!library) return "No templates installed. Download the community release or import an archive.";
    const parts = [
      library.source === "github" ? `nuclei-templates ${library.tag}` : "Imported archive",
      library.verified ? "checksum verified" : "not checksum-verified",
    ];
    if (library.commit) parts.push(`commit ${library.commit.slice(0, 12)}`);
    return `${parts.join(" · ")} — ${Number(status.templates).toLocaleString()} templates, ${Number(status.passive_supported).toLocaleString()} can run passively, ${Number(status.passive_enabled).toLocaleString()} on.`;
  }

  function supportLabel(meta) {
    if (meta.passive_supported) return PASSIVE_KINDS[meta.passive] || "Passive";
    const reason = (meta.reasons || [])[0];
    return reason ? `No: ${reason}` : "No";
  }

  function createController(options) {
    const shell = options.root;
    const doc = shell.ownerDocument;
    const fetcher = options.fetch || root.fetch.bind(root);
    const schedule = options.setTimeout || root.setTimeout.bind(root);
    const unschedule = options.clearTimeout || root.clearTimeout.bind(root);
    const ids = ["Status", "Progress", "Consent", "Download", "Check", "Import", "ImportFile", "Notice", "ScopeNote",
      "Search", "Severity", "Support", "EnabledOnly", "EnableShown", "DisableShown", "Meta", "Rows", "Detail",
      "PageMeta", "Previous", "Next"];
    const els = Object.fromEntries(ids.map((id) => [id, shell.querySelector(`#templates${id}`)]));
    const listeners = [];
    let visible = false;
    let status = null;
    let items = [];
    let total = 0;
    let page = 0;
    let selected = null;
    let timer = null;
    let searchTimer = null;
    let busy = false;
    let listRequest = 0;

    function on(element, event, callback) {
      element.addEventListener(event, callback);
      listeners.push(() => element.removeEventListener(event, callback));
    }
    function element(tag, className, text) {
      const node = doc.createElement(tag);
      if (className) node.className = className;
      if (text !== undefined) node.textContent = String(text);
      return node;
    }
    function notice(message, error = false) {
      els.Notice.textContent = message;
      els.Notice.classList.toggle("hidden", !message);
      els.Notice.classList.toggle("recon-error", error);
    }
    async function json(response, fallback) {
      if (!response.ok) {
        const text = (await response.text()).slice(0, 1000);
        throw new Error(text || `${fallback} (HTTP ${response.status})`);
      }
      return response.json();
    }
    function filters() {
      return { search: els.Search.value, severity: els.Severity.value, support: els.Support.value, enabledOnly: els.EnabledOnly.checked };
    }

    function renderStatus() {
      const working = status && BUSY_STATES.has(status.state);
      els.Status.textContent = libraryLine(status);
      let progress = "";
      if (status?.state === "downloading") {
        progress = status.received_bytes ? `Downloading… ${(status.received_bytes / 1048576).toFixed(1)} MiB` : "Contacting GitHub…";
      } else if (status?.state === "importing") {
        progress = "Reading and checking templates…";
      } else if (status?.state === "failed" && status.error) {
        progress = `The last download or import failed: ${status.error}`;
      }
      els.Progress.textContent = progress;
      els.Progress.classList.toggle("hidden", !progress);
      els.Progress.classList.toggle("recon-error", status?.state === "failed");
      els.Download.disabled = !!working || busy || !els.Consent.checked;
      els.Import.disabled = !!working || busy;
      els.Check.disabled = !!working || busy;
      const scopeless = (options.getScopeIncludeCount?.() || 0) === 0;
      els.ScopeNote.classList.toggle("hidden", !(scopeless && status?.passive_enabled > 0));
      // Bulk changes act on passive-capable templates only, so they are offered
      // only while the list shows exactly those.
      const bulk = !!status?.library && els.Support.value === "passive";
      els.EnableShown.disabled = !bulk || busy;
      els.DisableShown.disabled = !bulk || busy;
    }

    function renderList() {
      els.Rows.replaceChildren();
      if (!items.length) {
        const row = element("tr");
        const cell = element("td", "recon-empty", status?.library ? "No templates match." : "No templates installed.");
        cell.colSpan = 5;
        row.append(cell);
        els.Rows.append(row);
      }
      for (const meta of items) {
        const row = element("tr", meta.id === selected?.id ? "recon-selected" : "");
        const toggle = element("input");
        toggle.type = "checkbox";
        toggle.checked = !!meta.enabled;
        toggle.disabled = !meta.passive_supported || busy;
        toggle.dataset.templateToggle = meta.id;
        toggle.setAttribute("aria-label", `Run ${meta.name || meta.id} on captured traffic`);
        const toggleCell = element("td");
        toggleCell.append(toggle);
        const name = element("button", "recon-endpoint-button", meta.name || meta.id);
        name.type = "button";
        name.dataset.templateId = meta.id;
        const nameCell = element("td");
        nameCell.append(name, element("div", "recon-help", meta.id));
        row.append(toggleCell, element("td", "", meta.severity), nameCell, element("td", "", supportLabel(meta)),
          element("td", "", (meta.tags || []).slice(0, 6).join(", ")));
        els.Rows.append(row);
      }
      const first = total ? page * PAGE_SIZE + 1 : 0;
      els.PageMeta.textContent = total ? `${first.toLocaleString()}–${Math.min(total, first + items.length - 1).toLocaleString()} of ${total.toLocaleString()}` : "";
      els.Previous.disabled = page === 0;
      els.Next.disabled = (page + 1) * PAGE_SIZE >= total;
      els.Meta.textContent = status?.library ? `${total.toLocaleString()} shown by the filter` : "";
    }

    function field(list, label, value) {
      if (value === undefined || value === null || value === "" || (Array.isArray(value) && !value.length)) return;
      list.append(element("dt", "", label), element("dd", "", Array.isArray(value) ? value.join(", ") : value));
    }

    function renderDetail() {
      els.Detail.replaceChildren();
      if (!selected) {
        els.Detail.append(element("p", "recon-empty", "Select a template to see what it checks."));
        return;
      }
      els.Detail.append(element("h3", "", selected.name || selected.id));
      if (selected.description) els.Detail.append(element("p", "", selected.description));
      const fields = element("dl", "recon-detail-fields");
      field(fields, "Id", selected.id);
      field(fields, "Severity", selected.severity);
      field(fields, "Protocol", selected.protocol);
      field(fields, "Passive", selected.passive_supported ? `${PASSIVE_KINDS[selected.passive] || "Yes"}${selected.enabled ? " · on" : ""}` : "Cannot run passively");
      field(fields, "Active", selected.active_supported ? "Yes" : "Not available yet");
      field(fields, "Why not", selected.reasons);
      field(fields, "Risk tags", selected.risks);
      field(fields, "Tags", selected.tags);
      field(fields, "CVE", selected.cve);
      field(fields, "CWE", selected.cwe);
      field(fields, "Authors", selected.authors);
      field(fields, "Path", selected.path);
      els.Detail.append(fields);
      if (selected.references?.length) {
        els.Detail.append(element("h4", "", "References"));
        const list = element("ul", "recon-raw-urls");
        for (const reference of selected.references.slice(0, 20)) list.append(element("li", "", reference));
        els.Detail.append(list);
      }
      els.Detail.append(element("h4", "", "Template"));
      els.Detail.append(element("pre", "templates-yaml", selected.yaml || ""));
    }

    function clearTimer() { if (timer !== null) unschedule(timer); timer = null; }

    async function refreshStatus() {
      clearTimer();
      try {
        status = await json(await fetcher("/api/templates/library"), "Could not read the template library");
      } catch (error) {
        notice(error.message, true);
      }
      renderStatus();
      if (status && BUSY_STATES.has(status.state)) {
        if (visible) timer = schedule(refreshStatus, 1000);
        return;
      }
      await loadList();
    }

    async function loadList() {
      const request = ++listRequest;
      if (!status?.library) {
        items = [];
        total = 0;
        renderList();
        renderDetail();
        return;
      }
      try {
        const result = await json(await fetcher(listQuery(filters(), page)), "Could not list templates");
        if (request !== listRequest) return;
        items = Array.isArray(result.items) ? result.items : [];
        total = Number(result.total) || 0;
        renderList();
      } catch (error) {
        if (request === listRequest) notice(error.message, true);
      }
    }

    async function open(id) {
      try {
        selected = await json(await fetcher(`/api/templates/${encodeURIComponent(id)}`), "Could not open the template");
      } catch (error) {
        notice(error.message, true);
        return;
      }
      renderList();
      renderDetail();
    }

    async function act(work) {
      if (busy) return;
      busy = true;
      renderStatus();
      try {
        await work();
      } catch (error) {
        notice(error.message || "The request failed.", true);
      } finally {
        busy = false;
        await refreshStatus();
      }
    }

    function setPassive(body, describe) {
      return act(async () => {
        const result = await json(await fetcher("/api/templates/passive", {
          method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body),
        }), "Could not save the passive templates");
        const count = Number(result.passive_enabled) || 0;
        notice(`${describe} ${count.toLocaleString()} passive ${count === 1 ? "template is" : "templates are"} on. Rescan in Capture ▸ Findings applies the change to traffic already captured.`);
        if (selected && Array.isArray(body.ids) && body.ids.includes(selected.id)) await open(selected.id);
      });
    }

    on(els.Consent, "change", renderStatus);
    on(els.Download, "click", () => act(async () => {
      if (!els.Consent.checked) throw new Error("Confirm the download first.");
      await json(await fetcher("/api/templates/library/download", {
        method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ acknowledged: true }),
      }), "Could not start the download");
      notice("");
    }));
    on(els.Check, "click", () => act(async () => {
      const result = await json(await fetcher("/api/templates/library/check", { method: "POST" }), "Could not reach GitHub");
      notice(result.update_available
        ? `Release ${result.latest} is available${result.current ? ` (installed: ${result.current})` : ""}.`
        : `Release ${result.latest} is installed and current.`);
    }));
    on(els.Import, "click", () => els.ImportFile.click());
    on(els.ImportFile, "change", () => {
      const file = els.ImportFile.files?.[0];
      els.ImportFile.value = "";
      if (!file) return;
      void act(async () => {
        await json(await fetcher("/api/templates/library/import", {
          method: "POST", headers: { "Content-Type": "application/gzip" }, body: file,
        }), "Could not import the archive");
        notice("");
      });
    });
    on(els.Search, "input", () => {
      if (searchTimer !== null) unschedule(searchTimer);
      searchTimer = schedule(() => { searchTimer = null; page = 0; void loadList(); }, 250);
    });
    for (const control of [els.Severity, els.Support, els.EnabledOnly]) {
      on(control, "change", () => { page = 0; renderStatus(); void loadList(); });
    }
    on(els.Previous, "click", () => { page = Math.max(0, page - 1); void loadList(); });
    on(els.Next, "click", () => { page += 1; void loadList(); });
    on(els.EnableShown, "click", () => setPassive(passiveRequest(filters(), true), "Turned on the passive templates the filter shows."));
    on(els.DisableShown, "click", () => setPassive(passiveRequest(filters(), false), "Turned off the passive templates the filter shows."));
    on(els.Rows, "click", (event) => {
      const toggle = event.target.closest("[data-template-toggle]");
      if (toggle) {
        void setPassive({ ids: [toggle.dataset.templateToggle], enabled: toggle.checked }, toggle.checked ? "Turned it on." : "Turned it off.");
        return;
      }
      const name = event.target.closest("[data-template-id]");
      if (name) void open(name.dataset.templateId);
    });

    function setVisible(next) {
      const changed = visible !== !!next;
      visible = !!next;
      if (!visible) clearTimer();
      else if (changed) void refreshStatus();
    }

    renderStatus();
    renderList();
    renderDetail();
    return { setVisible, refresh: refreshStatus, dispose() { clearTimer(); listeners.forEach((remove) => remove()); } };
  }

  const api = { createController, listQuery, passiveRequest, libraryLine, supportLabel, PAGE_SIZE };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.SniperTemplates = api;
})(typeof window !== "undefined" ? window : globalThis);
