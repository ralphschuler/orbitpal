# OBS browser source policy

The loopback OBS server applies these response headers to HTML, frontend assets, imported avatars, speech/emote audio, event streams, playback acknowledgements and errors:

- `Content-Security-Policy`: network resources come only from the application's exact origin. Embedded data/blob images, audio and glTF buffers, plus data fonts, are allowed. Inline style properties support the character presentation; inline scripts, evaluation, workers, plugins, child frames and form submissions are blocked. Other HTTP hosts/ports, cloud endpoints and native IPC origins are not allowed.
- `Referrer-Policy: no-referrer`: browser requests do not disclose the OBS URL, whose query contains its session access link.
- `X-Content-Type-Options: nosniff`: browsers use the declared resource MIME type rather than interpreting arbitrary content as a script or stylesheet.

The browser source uses same-origin `EventSource` and fetch calls for events, model files and speech/emote playback. Its dynamic loopback port needs no broad port wildcard: CSP `'self'` matches the exact origin used to open that source. Native WebView permissions remain configured separately. No HTTPS upgrade or HSTS header is added to the local HTTP server.

Imported GLB/VRM files still use the shared bounded native resource validator. Import rejects external image/buffer references; the OBS asset route also checks legacy imported files before serving them. The browser policy adds a second boundary and does not replace format validation, session checks, per-bot routing or audio cancellation. Bundled avatars and embedded textures remain supported. Future compressed-model decoders must review worker/WASM permissions explicitly; this policy does not enable them preemptively.

Local HTTP fixtures cover the actual response middleware with HTML/JavaScript, avatar bytes, audio JSON, SSE, playback POSTs, authentication errors and missing routes. They verify headers, MIME types, status codes, body data and streaming delivery. These fixtures do not claim a live OBS rendering or browser audio-activation check.

References: [CSP connect-src](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/connect-src), [Referrer-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Referrer-Policy), [X-Content-Type-Options](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/X-Content-Type-Options).
