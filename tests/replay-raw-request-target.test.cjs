const assert = require('node:assert/strict');
const test = require('node:test');
const { loadFunctions } = require('./frontend-test-helpers.cjs');

const context = loadFunctions([
  'parseEditableRawRequest', 'splitRawHttpMessage', 'parseReplayHttpVersionToken',
  'normalizeReplayHttpVersion', 'headerValue', 'headerNameEquals', 'normalizedHeaders',
  'editableRequestBodyLength', 'validateRawHttpBodyFraming',
  'httpRequestAuthoritiesEquivalent', 'normalizeHttpRequestAuthority',
  'authorityToTargetState', 'stripIpv6Brackets', 'normalizePortValue', 'defaultHttpPortForScheme',
], { URL, TextEncoder, document: { getElementById: () => null } });

function parse(target, host = '', method = 'GET') {
  return context.parseEditableRawRequest(
    `${method} ${target} HTTP/1.1${host ? `\r\nHost: ${host}` : ''}\r\n\r\n`,
    { scheme: 'https', host: 'fallback.example.com' },
  );
}

for (const path of [
  '/a/../b', '/a/./b', '/a/%2e%2e/b', '/a/%2E./b', '/a/.%2e/b', '/%2e/a',
  '//other.example.com/a/../b', '/a///b//', '/?', '/a/../b?',
  '/%2f/%2F/%41/%7e?q=%2f&next=../x&empty=', '/a?x=/../b&x=2', '/a@b?q=user@example.com',
]) {
  test(`absolute-form preserves raw target ${path}`, () => {
    assert.equal(parse(`https://example.com${path}`).path, path);
  });
  test(`origin-form remains unchanged ${path}`, () => {
    assert.equal(parse(path, 'example.com').path, path);
  });
}

for (const [target, scheme, host, path] of [
  ['http://EXAMPLE.com:80', 'http', 'example.com', '/'],
  ['HTTPS://EXAMPLE.COM:443?', 'https', 'example.com', '/?'],
  ['http://example.com:8080?x=1', 'http', 'example.com:8080', '/?x=1'],
  ['https://[::1]:8443/a/../b', 'https', '[::1]:8443', '/a/../b'],
]) {
  test(`absolute authority and scheme stay validated: ${target}`, () => {
    const result = parse(target);
    assert.equal(result.scheme, scheme);
    assert.equal(result.host, host);
    assert.equal(result.path, path);
  });
}

test('absolute-form retains Host equivalence checks', () => {
  assert.equal(parse('http://EXAMPLE.com:80/a/../b', 'example.com').path, '/a/../b');
  assert.equal(parse('https://example.com/a', 'EXAMPLE.COM:443').path, '/a');
  assert.throws(() => parse('https://example.com/a', 'other.example.com'), /does not match Host/);
  assert.throws(() => parse('https://example.com/a', 'example.com:8443'), /does not match Host/);
});

for (const target of ['https://user@example.com/a', 'https://user:pass@example.com/a', 'http://@example.com/a', 'https://:@example.com/a']) {
  test(`credentials remain rejected: ${target}`, () => {
    assert.throws(() => parse(target), /must not include credentials/);
  });
}
for (const target of ['https://example.com/a#fragment', 'https://example.com/a#']) {
  test(`fragments are rejected: ${target}`, () => {
    assert.throws(() => parse(target), /must not include a fragment/);
  });
}
for (const target of [
  'https://', 'https://?x=1', 'https://example.com:abc/a', 'https://example.com:65536/a',
  'https://[::1/a', 'https:///example.com/a', 'https://example.com\\other/a',
  'https://example.com/a\u0000', 'https://example.com/a\u007f', 'https://example.com/a b',
]) {
  test(`malformed absolute target is rejected: ${JSON.stringify(target)}`, () => {
    assert.throws(() => parse(target));
  });
}

test('asterisk and CONNECT behavior remains unchanged', () => {
  assert.equal(parse('*', 'example.com', 'OPTIONS').path, '*');
  assert.throws(() => parse('example.com:443', 'example.com', 'CONNECT'), /not supported by Replay/);
});
