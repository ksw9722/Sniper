/* Draft-local setup helpers. The existing editor parser owns HTTP validation;
   this module never sends requests or reads retained traffic. */
(function (root) {
  "use strict";

  const MARKER = "$payload$";
  const limits = Object.freeze({
    payloads: 5000,
    payloadBytes: 64 * 1024,
    totalPayloadBytes: 4 * 1024 * 1024,
    markers: 128,
    expandedRequestBytes: 4 * 1024 * 1024,
    candidates: 128,
    samples: 3,
    sampleBytes: 128 * 1024,
  });
  const encoder = new TextEncoder();
  const bytes = value => encoder.encode(value).length;
  const presets = Object.freeze([
    Object.freeze({ id: "text-boundaries", label: "Text boundaries", payloads: Object.freeze(["", " ", "a", "A", "0", "é", "😀", "a".repeat(256)]) }),
    Object.freeze({ id: "number-boundaries", label: "Number boundaries", payloads: Object.freeze(["", "-1", "0", "1", "127", "128", "255", "256", "2147483647", "2147483648"]) }),
  ]);

  function payloadLines(text) {
    const normalized = String(text ?? "").replace(/\r\n/g, "\n").replace(/\r/g, "\n");
    if (normalized.length === 0) return [];
    const lines = normalized.split("\n");
    if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
    return lines;
  }

  function payloadText(payloads) {
    if (!Array.isArray(payloads) || payloads.some(value => typeof value !== "string" || /[\r\n]/.test(value))) {
      throw new Error("The payload editor needs one single-line string per payload.");
    }
    if (!payloads.length) return "";
    // A final newline is an editor delimiter, not an additional empty payload.
    return payloads.join("\n") + (payloads[payloads.length - 1] === "" ? "\n" : "");
  }

  function validatePayloads(input) {
    if (!Array.isArray(input) || input.some(value => typeof value !== "string")) {
      return { valid: false, errors: ["Payloads must be a list of strings."], count: 0, totalBytes: 0, maxPayloadBytes: 0, payloads: [] };
    }
    // src/fuzzer.rs strips exactly one trailing CR, including API-supplied lists.
    const payloads = input.map(value => value.endsWith("\r") ? value.slice(0, -1) : value);
    const errors = [];
    if (!payloads.length) errors.push("Fuzzer needs at least one payload.");
    if (payloads.length > limits.payloads) errors.push(`Fuzzer payload count cannot exceed ${limits.payloads}.`);
    let totalBytes = 0, maxPayloadBytes = 0, oversizedIndex = -1;
    for (let index = 0; index < payloads.length; index += 1) {
      const size = bytes(payloads[index]);
      totalBytes += size;
      maxPayloadBytes = Math.max(maxPayloadBytes, size);
      if (size > limits.payloadBytes && oversizedIndex === -1) oversizedIndex = index;
    }
    if (oversizedIndex !== -1) errors.push(`Fuzzer payload ${oversizedIndex} cannot exceed ${limits.payloadBytes} bytes.`);
    if (totalBytes > limits.totalPayloadBytes) errors.push(`Fuzzer payloads cannot exceed ${limits.totalPayloadBytes} total bytes.`);
    return { valid: !errors.length, errors, count: payloads.length, totalBytes, maxPayloadBytes, payloads };
  }

  function rangeInteger(value) {
    if ((typeof value !== "number" && typeof value !== "string")
      || (typeof value === "string" && !/^[+-]?\d+$/.test(value.trim()))
      || !Number.isSafeInteger(Number(value))) {
      throw new Error("Range start, end, and step must be safe whole numbers.");
    }
    return BigInt(Number(value));
  }

  function numericRange(start, end, step = 1) {
    const first = rangeInteger(start), last = rangeInteger(end), stride = rangeInteger(step);
    if (stride === 0n) throw new Error("Range step must not be zero.");
    if ((first < last && stride < 0n) || (first > last && stride > 0n)) {
      throw new Error("Range step must move from start toward end.");
    }
    const count = (last - first) / stride + 1n;
    if (count > BigInt(limits.payloads)) throw new Error(`Range cannot exceed ${limits.payloads} payloads.`);
    return Array.from({ length: Number(count) }, (_, index) => String(first + BigInt(index) * stride));
  }

  function credentialName(name) {
    // This is a conservative suggestion filter, not a claim to detect secrets.
    // Do not decode names: both labels and positions describe the literal draft.
    return /(?:auth|password|passwd|pwd|secret|token|cookie|session|credential|api[-_]?key|access[-_]?key|private[-_]?key|csrf|xsrf)/i.test(name)
      || /^(?:key|sid|jwt)$/i.test(name);
  }

  function addValueCandidates(raw, start, end, kind, output) {
    const occurrences = new Map();
    let cursor = start;
    while (cursor < end && output.length < limits.candidates) {
      const ampersand = raw.indexOf("&", cursor);
      const fieldEnd = ampersand === -1 || ampersand > end ? end : ampersand;
      const equals = raw.indexOf("=", cursor);
      if (equals >= cursor && equals < fieldEnd) {
        const name = raw.slice(cursor, equals);
        const ordinal = (occurrences.get(name) || 0) + 1;
        occurrences.set(name, ordinal);
        if (!credentialName(name) && !raw.slice(equals + 1, fieldEnd).includes(MARKER)) {
          const shortName = name.length > 80 ? `${name.slice(0, 80)}…` : name;
          output.push({ kind, label: `${kind === "query" ? "Query" : "Form"}: ${shortName || "(unnamed)"} (${ordinal})`, start: equals + 1, end: fieldEnd });
        }
      }
      if (fieldEnd === end) break;
      cursor = fieldEnd + 1;
    }
  }

  function candidates(raw, { bodyEncoding = "utf8", previewTruncated = false } = {}) {
    raw = String(raw ?? "");
    const output = [];
    // Caller must first validate with parseEditableRawRequest. This scanner only
    // locates spans in the original string; it does not construct an HTTP model.
    const firstLineEnd = raw.search(/[\r\n]/);
    const firstLine = raw.slice(0, firstLineEnd === -1 ? raw.length : firstLineEnd);
    const target = /^\S+[\t ]+(\S+)/.exec(firstLine);
    if (target) {
      const targetStart = target[0].length - target[1].length;
      const question = target[1].indexOf("?");
      const fragment = target[1].indexOf("#");
      if (question !== -1 && (fragment === -1 || question < fragment)) {
        addValueCandidates(raw, targetStart + question + 1, targetStart + (fragment === -1 ? target[1].length : fragment), "query", output);
      }
    }
    const boundary = /\r?\n\r?\n/.exec(raw);
    if (!boundary || bodyEncoding !== "utf8" || previewTruncated) return output;
    const head = raw.slice(0, boundary.index);
    const contentTypes = [...head.matchAll(/(?:^|\r?\n)content-type[\t ]*:[\t ]*([^\r\n]*)/gi)];
    if (contentTypes.length !== 1 || !/^application\/x-www-form-urlencoded(?:\s*;|\s*$)/i.test(contentTypes[0][1])) return output;
    if (/(?:^|\r?\n)content-encoding[\t ]*:[\t ]*(?!identity[\t ]*(?:\r?$))\S/im.test(head)) return output;
    const bodyStart = boundary.index + boundary[0].length;
    if (/[\u0000-\u001f\u007f\ufffd]/.test(raw.slice(bodyStart))) return output;
    addValueCandidates(raw, bodyStart, raw.length, "form", output);
    return output;
  }

  function replaceSelection(raw, start, end) {
    raw = String(raw ?? "");
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < start || end > raw.length) {
      throw new Error("Select a value in the current request draft.");
    }
    const splitsCharacter = offset => offset > 0 && offset < raw.length
      && /[\uD800-\uDBFF]/.test(raw[offset - 1]) && /[\uDC00-\uDFFF]/.test(raw[offset]);
    if (splitsCharacter(start) || splitsCharacter(end)) throw new Error("Select a complete character.");
    for (let index = raw.indexOf(MARKER); index !== -1; index = raw.indexOf(MARKER, index + MARKER.length)) {
      if ((start < index + MARKER.length && end > index)
        || (start === end && start > index && start < index + MARKER.length)) {
        throw new Error("Selection overlaps an existing $payload$ marker. Select an unmarked value.");
      }
    }
    return raw.slice(0, start) + MARKER + raw.slice(end);
  }

  const isHeader = (header, name) => header.name.toLowerCase() === name;
  const markerCount = value => value.split(MARKER).length - 1;
  const literalReplace = (value, payload) => value.split(MARKER).join(payload);

  function templateShape(template) {
    if (!template || ["scheme", "host", "method", "path", "body"].some(key => typeof template[key] !== "string")
      || !Array.isArray(template.headers) || template.headers.some(header => !header || typeof header.name !== "string" || typeof header.value !== "string")) {
      throw new Error("Preview needs a request validated by the request editor.");
    }
  }

  function base64Length(value) {
    // Rust STANDARD decoding requires canonical padding and zero unused bits;
    // atob and Buffer alone also accept encodings that the backend rejects.
    if (value.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(value)) return null;
    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if (value.endsWith("==") && (alphabet.indexOf(value[value.length - 3]) & 15)) return null;
    if (value.endsWith("=") && !value.endsWith("==") && (alphabet.indexOf(value[value.length - 2]) & 3)) return null;
    return value.length / 4 * 3 - (value.endsWith("==") ? 2 : value.endsWith("=") ? 1 : 0);
  }

  function expandTemplate(template, payload) {
    templateShape(template);
    if (typeof payload !== "string") throw new Error("Payload must be a string.");
    const request = { ...template, headers: template.headers.map(header => ({ ...header })) };
    const hasHost = request.headers.some(header => isHeader(header, "host"));
    if (!hasHost) request.host = literalReplace(request.host, payload);
    for (const key of ["method", "path", "body"]) request[key] = literalReplace(request[key], payload);
    for (const header of request.headers) {
      header.name = literalReplace(header.name, payload);
      header.value = literalReplace(header.value, payload);
    }
    const host = request.headers.find(header => isHeader(header, "host"));
    if (host) request.host = host.value;
    if (request.headers.some(header => isHeader(header, "content-length"))) {
      const length = request.body_encoding === "base64" ? base64Length(request.body) : bytes(request.body);
      if (length !== null) {
        request.headers = request.headers.filter(header => !isHeader(header, "content-length"));
        request.headers.push({ name: "Content-Length", value: String(length) });
      }
    }
    return request;
  }

  function requestBudget(template) {
    const hasHost = template.headers.some(header => isHeader(header, "host"));
    const fields = [template.method, template.path, template.body];
    if (!hasHost) fields.push(template.host);
    for (const header of template.headers) fields.push(header.name, header.value);
    const markers = fields.reduce((sum, value) => sum + markerCount(value), 0);
    // Match expanded_request_size, including its scheme accounting and original
    // host bytes when Host takes precedence. These are not outbound wire bytes.
    const budgetMarkers = markers + markerCount(template.scheme);
    const base = fields.reduce((sum, value) => sum + bytes(value), bytes(template.scheme) + (hasHost ? bytes(template.host) : 0));
    return { markers, size: payloadBytes => base - budgetMarkers * MARKER.length + budgetMarkers * payloadBytes };
  }

  function authorityParts(value) {
    const bracketed = /^(\[[^\]]+\])(?::(\d+))?$/.exec(value);
    const plain = /^([^:]+)(?::(\d+))?$/.exec(value);
    const match = bracketed || plain;
    return match ? { host: match[1], port: match[2] || "" } : { host: value.includes(":") ? `[${value}]` : value, port: "" };
  }

  function effectiveTarget(template, target) {
    const hostHeader = template.headers.find(header => isHeader(header, "host"));
    const request = authorityParts((hostHeader ? hostHeader.value : template.host).trim());
    const scheme = String(target?.scheme || template.scheme).trim();
    const destination = target?.host ? authorityParts(String(target.host).trim()) : request;
    const port = String(target?.port || destination.port || request.port || (scheme.toLowerCase() === "http" ? "80" : "443"));
    return `${scheme}://${destination.host}:${port}`;
  }

  function preflight(template, payloads, { target = null, httpVersion = null } = {}) {
    templateShape(template);
    const validation = validatePayloads(payloads);
    const budget = requestBudget(template);
    const errors = validation.errors.slice();
    const warnings = ["Local expansion preview only. Runtime request rewriting can change the outbound request; the server validates requests before running."];
    if (!budget.markers) errors.push("Request template is missing $payload$ markers.");
    if (budget.markers > limits.markers) errors.push(`Fuzzer request templates cannot contain more than ${limits.markers} $payload$ markers.`);
    if (template.preview_truncated) errors.push("The request body preview is truncated. Load or supply the complete body first.");
    const maxExpandedRequestBytes = budget.size(validation.maxPayloadBytes);
    if (validation.count && maxExpandedRequestBytes > limits.expandedRequestBytes) {
      errors.push(`An expanded request exceeds the ${limits.expandedRequestBytes} byte limit.`);
    }
    if (template.body_encoding === "base64") warnings.push("The body is base64 encoded. Payload substitution is literal encoded text, not decoded binary data.");
    const hostHeader = template.headers.find(header => isHeader(header, "host"));
    if (markerCount(hostHeader ? hostHeader.value : template.host)) {
      warnings.push("Host markers can change the request authority by payload. Check the sample requests and connection target.");
    }
    const samples = [];
    let sampleBytes = 0, samplesOmitted = 0;
    if (!errors.length) {
      for (let index = 0; index < Math.min(limits.samples, validation.count); index += 1) {
        const payload = validation.payloads[index];
        const estimatedBytes = budget.size(bytes(payload));
        // Host occurs both in the request model and headers after expansion.
        // Leave room for that copy and a normalized Content-Length record.
        const previewBytes = 2 * estimatedBytes + bytes(payload) + 256 + 32 * template.headers.length;
        if (sampleBytes + previewBytes > limits.sampleBytes) { samplesOmitted += 1; continue; }
        const request = expandTemplate(template, payload);
        samples.push({ index, payload, request, estimatedBytes, effectiveTarget: effectiveTarget(request, target) });
        sampleBytes += previewBytes;
      }
    }
    if (samplesOmitted) warnings.push(`${samplesOmitted} sample(s) omitted to keep the local preview bounded.`);
    return {
      valid: !errors.length, errors, warnings,
      markerCount: budget.markers, payloadCount: validation.count, requestCount: validation.count,
      totalPayloadBytes: validation.totalBytes, maxPayloadBytes: validation.maxPayloadBytes,
      maxExpandedRequestBytes, effectiveTarget: effectiveTarget(template, target),
      httpVersion: httpVersion || template.http_version || null, limits, samples, samplesOmitted,
    };
  }

  const api = Object.freeze({ MARKER, limits, presets, payloadLines, payloadText, validatePayloads, numericRange, candidates, replaceSelection, expandTemplate, preflight });
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) root.SniperFuzzerWorkflow = api;
})(typeof window !== "undefined" ? window : null);
