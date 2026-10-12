// Backend-shaped, synthetic retained-preview envelopes. No requests or captures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const context = vm.createContext({ URL });
vm.runInContext(fs.readFileSync(path.join(__dirname, '../web/service-map.js'), 'utf8'), context);
const map = context.SniperServiceMap;
const SESSION = '11111111-1111-4111-8111-111111111111';
const TOKEN = `v1.${SESSION}.22222222-2222-4222-8222-222222222222.1.33333333-3333-4333-8333-333333333333.44444444-4444-4444-8444-444444444444`;
const ROW = { origin: 'https://example.com', sequence: '1', started_at: '2026-10-12T00:00:00Z' };
function scan(overrides = {}) {
  return { schema_version: 1, session_id: SESSION, evidence_id: TOKEN, available: true, status: 'scanned', complete: true, reasons: [],
    source_origin: ROW.origin, source_sequence: ROW.sequence, source_started_at: ROW.started_at,
    retained_preview_bytes: 100, scanned_bytes: 100, capture_preview_truncated: false, scan_prefix_truncated: false,
    candidates_scanned: 2, omitted_reference_occurrences: 0,
    references: [{ target_origin: 'https://asset.example.com', occurrences: 2, preview_byte_offsets: [0, 50], omitted_offsets: 0 }],
    limits: { ...map.BODY_REFERENCE_LIMITS }, coverage: 'literal_absolute_http_urls', offset_basis: 'retained_utf8_preview', ...overrides };
}
function missing(reason = 'record_missing', status = 'unavailable') {
  const payload = scan({ available: false, status, complete: false, reasons: [reason], scanned_bytes: 0, candidates_scanned: 0, references: [] });
  for (const field of ['source_origin', 'source_sequence', 'source_started_at', 'retained_preview_bytes']) delete payload[field];
  return payload;
}
const normalize = payload => map.normalizeBodyReferences(payload, SESSION, TOKEN, ROW);
const partial = overrides => scan({ status: 'partial', complete: false, ...overrides });

test('body contract preserves zero-reference scans, unknown-field dropping and long body origins', () => {
  const empty = normalize(scan({ retained_preview_bytes: 0, scanned_bytes: 0, candidates_scanned: 0, references: [], raw_body: 'SYNTHETIC_PRIVATE_VALUE' }));
  assert.equal(empty.status, 'scanned');
  assert.doesNotMatch(JSON.stringify(empty), /SYNTHETIC_PRIVATE_VALUE|raw_body/);
  const payload = scan();
  payload.references[0].target_origin = `https://${'a'.repeat(600)}.example.com`;
  assert.equal(normalize(payload).references[0].target_origin, payload.references[0].target_origin);
});

for (const [name, payload] of [
  ['capture truncation', partial({ reasons: ['capture_preview_truncated'], capture_preview_truncated: true })],
  ['resident prefix with UTF-8 boundary', partial({ reasons: ['scan_prefix_truncated'], scan_prefix_truncated: true, scanned_bytes: 262141, retained_preview_bytes: 3000000 })],
  ['candidate cap', partial({ reasons: ['candidate_limit'], candidates_scanned: 4096, retained_preview_bytes: 262144, scanned_bytes: 262144 })],
  ['oversized candidate', partial({ reasons: ['candidate_bytes'] })],
  ['offset cap', partial({ reasons: ['offset_limit'], candidates_scanned: 6, references: [{ target_origin: 'https://asset.example.com', occurrences: 6, preview_byte_offsets: [0, 10, 20, 30, 40], omitted_offsets: 1 }] })],
  ['origin cap', partial({ reasons: ['origin_limit'], candidates_scanned: 101, retained_preview_bytes: 10000, scanned_bytes: 10000, omitted_reference_occurrences: 1,
    references: Array.from({ length: 100 }, (_, i) => ({ target_origin: `https://r${i}.example.com`, occurrences: 1, preview_byte_offsets: [i * 50], omitted_offsets: 0 })) })],
]) test(`body contract preserves legitimate ${name}`, () => {
  assert.equal(normalize(payload).status, 'partial');
});

const acquisitionReasons = ['invalid_evidence_id', 'session_mismatch', 'stale_generation', 'record_missing', 'record_changed', 'body_identity_unverifiable',
  'body_storage_unavailable', 'body_read_failed', 'serialized_row_limit', 'malformed_retained_row', 'response_unavailable', 'retained_preview_unavailable',
  'unsupported_metadata', 'metadata_limit', 'unsupported_content_type', 'unsupported_body_encoding', 'unsupported_content_encoding', 'body_scan_failed',
  'inactive_session', 'session_source_changed', 'metadata_unavailable'];
test('body contract preserves fixed acquisition reasons and omitted or null optional fields', () => {
  for (const reason of acquisitionReasons) {
    const payload = missing(reason, reason.startsWith('unsupported_') || reason === 'metadata_limit' ? 'unsupported' : 'unavailable');
    assert.equal(normalize(payload).reasons[0], reason);
    for (const field of ['source_origin', 'source_sequence', 'source_started_at', 'retained_preview_bytes']) payload[field] = null;
    assert.equal(normalize(payload).reasons[0], reason);
  }
});

const invalid = [
  ['available acquisition failure', partial({ reasons: ['record_missing'] })],
  ['unknown partial reason', partial({ reasons: ['synthetic_private_value'] })],
  ['duplicate partial reason', partial({ reasons: ['candidate_bytes', 'candidate_bytes'] })],
  ['unsorted partial reasons', partial({ reasons: ['capture_preview_truncated', 'candidate_bytes'], capture_preview_truncated: true })],
  ['candidate cap below bound', partial({ reasons: ['candidate_limit'] })],
  ['oversized candidate without candidates', partial({ reasons: ['candidate_bytes'], candidates_scanned: 0, references: [] })],
  ['capture reason without flag', partial({ reasons: ['capture_preview_truncated'] })],
  ['capture flag without reason', partial({ reasons: ['candidate_bytes'], capture_preview_truncated: true })],
  ['prefix reason without flag', partial({ reasons: ['scan_prefix_truncated'] })],
  ['prefix flag without reason', partial({ reasons: ['candidate_bytes'], scan_prefix_truncated: true, scanned_bytes: 262144, retained_preview_bytes: 3000000 })],
  ['prefix flag without omitted bytes', partial({ reasons: ['scan_prefix_truncated'], scan_prefix_truncated: true })],
  ['prefix stop below bound', partial({ reasons: ['scan_prefix_truncated'], scan_prefix_truncated: true, retained_preview_bytes: 101 })],
  ['missing prefix flag', partial({ reasons: ['candidate_bytes'], retained_preview_bytes: 101 })],
  ['origin cap without omitted references', partial({ reasons: ['origin_limit'] })],
  ['origin omissions before origin cap', partial({ reasons: ['origin_limit'], candidates_scanned: 3, omitted_reference_occurrences: 1 })],
  ['origin omissions without reason', partial({ reasons: ['candidate_bytes'], candidates_scanned: 3, omitted_reference_occurrences: 1 })],
  ['offset cap without omitted offsets', partial({ reasons: ['offset_limit'] })],
  ['offset omissions without reason', partial({ reasons: ['candidate_bytes'], candidates_scanned: 6, references: [{ target_origin: 'https://asset.example.com', occurrences: 6, preview_byte_offsets: [0, 10, 20, 30, 40], omitted_offsets: 1 }] })],
  ['unavailable without reason', { ...missing(), reasons: [] }],
  ['unavailable with partial reason', missing('candidate_limit')],
  ['unsupported with unavailable reason', missing('record_missing', 'unsupported')],
  ['unavailable with unsupported reason', missing('unsupported_content_type')],
  ['unavailable with duplicate reasons', { ...missing(), reasons: ['record_missing', 'record_missing'] }],
  ['unavailable with unknown reason', missing('synthetic_private_value')],
  ['unavailable with capture flag', { ...missing(), capture_preview_truncated: true }],
  ['unavailable with prefix flag', { ...missing(), scan_prefix_truncated: true }],
];
for (const [field, value] of Object.entries({ source_origin: ROW.origin, source_sequence: ROW.sequence, source_started_at: ROW.started_at, retained_preview_bytes: 100 })) {
  invalid.push([`unavailable with ${field}`, { ...missing(), [field]: value }]);
}
for (const [name, payload] of invalid) test(`body contract rejects ${name} with fixed diagnostic`, () => {
  assert.throws(() => normalize(payload), { message: 'Invalid retained body-reference response' });
});
