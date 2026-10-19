# Decoded value preview

Hover over an encoded token in an HTTP text editor for 300 ms to see a local,
read-only decoded preview. Each successful decoding layer is labeled. Move the
pointer into the tooltip to read or select it; Escape closes it. Moving away,
clicking elsewhere, scrolling the editor, changing selection/content, replacing
the selected record, resizing, or leaving the editor dismisses the preview.

For keyboard access, place the cursor in a token (or select the exact value),
then Tab to the editor's **Decode** button and press Enter or Space. This uses
normal button navigation and reserves no browser/OS shortcut. While the button
keeps focus, use Up/Down, Page Up/Down or Home/End to scroll a long preview.
The button becomes
visible when the editor is hovered or contains keyboard focus. Unsupported
keyboard selections receive a short explanatory message.

## Supported text

- URL percent encoding, including UTF-8. Literal `+` stays `+`: the preview does
  not infer form encoding without knowing the field's context.
- Canonical Base64 and Base64URL, with or without padding, at least eight input
  characters long. JWT header/payload segments can be inspected individually;
  decoding is not signature validation.
- Up to three nested layers, such as URL-encoded Base64.

Detection is intentionally conservative. Invalid UTF-8, malformed escapes or
padding, control/bidirectional-control characters, whitespace-only output and
binary data have no preview. Short Base64 values, arbitrary hex, HTML entities,
JavaScript escapes, compression and wrapped/multiline Base64 are not guessed.
The preview can occasionally recognize an ordinary string as encoded text;
encoded-looking text is never treated as proof of a vulnerability or a credential.

One token or explicit selection is limited to 4,096 UTF-16 code units. Each
result is limited to 2,048 code units, with at most three layers. Oversized values
are rejected rather than partially decoded. Hover detection reads only a small
slice around the pointer; it does not scan or decode the whole packet.

## Supported panes

The shared CodeMirror HTTP text view enables previews in:

- HTTP History request and response, including Raw, Pretty and Modified text
- HTTP Replay request and response
- HTTP Intercept request and response
- Findings request and response detail
- Fuzzer request and captured-result request/response detail
- WebSocket HTTP handshake text

Hex, diff and rendered-response views are excluded. WebSocket frame viewers,
legacy plain-text/textarea fallbacks, Map/Recon custom views, and Decoder tool
inputs are not covered by this feature.

Decoded strings are inserted using `textContent`, never HTML. The feature does
not edit the source, add undo entries, alter captured bytes, send requests,
contact targets, save values, log content or evaluate decoded code.

## Regression checks

`node --test tests/encoded-preview-*.test.cjs` exercises decoding boundaries and
production plugin event/update callbacks against the shipped CodeMirror state
implementation, using synthetic text and a minimal event-capable DOM adapter.
These tests cover dismissal, keyboard button activation, selection/undo
preservation, identical-text replacement ownership, stale timers and cleanup.
They do not substitute for native WebKit/WebView2 layout or screen-reader QA.
