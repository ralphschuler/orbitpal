# External transport limits

OrbitPal rejects oversized external output while reading it. An incomplete line,
missing HTTP Content-Length or an endless sequence of SSE comments cannot bypass
the byte budgets.

| Transport | Budget |
| --- | --- |
| MCP stdio | 8 MiB of stdout per request, including notifications and unmatched messages; at most 1,000 messages |
| MCP HTTP JSON / SSE | 8 MiB per response, including SSE framing, comments and unmatched events |
| MCP tool catalog | 1,000 tools and 8 MiB of serialized catalog data across at most 20 pages, including the built-in resource/prompt adapters |
| Codex app-server | 4 MiB of stdout and 64 KiB of stderr across one worker connection |
| Claude Code and CLI status commands | 4 MiB of stdout and 64 KiB of stderr per process |
| Explicitly enabled Skill scripts | 128,000 bytes of stdout and 32,000 bytes of stderr per process |

MCP stdio stderr is discarded to the operating system's null sink and is never
retained by the app.

These are transport byte limits, not measured resident-memory figures. Parsing
JSON, decoding text and holding a bounded read buffer also use memory. The fixed
stdio read buffer may prefetch up to 8 KiB; body readers inspect at most one byte
beyond their allowance before reporting overflow. HTTP chunks are checked before
they are copied into the response buffer. Declared HTTP sizes may reject a
response early, but streaming checks remain mandatory.
Encoded JSON includes any base64 image data. The separate downstream image and
text limits also apply; they do not increase the transport allowance.

An oversized MCP transport response closes the affected stdio process and marks
the connection unusable until the operator reconnects it. Catalog import failures
discard the new session. Failed CLI/Skill captures terminate and reap the child.
Codex stderr is drained concurrently and its overflow interrupts a silent stdout
read or a blocked stdin write. Existing user cancellation and request deadlines
remain in force; dropping a CLI/Skill operation terminates its worker.
An interrupted MCP stdio read retains its incomplete line for the next request
and counts that prefix against the next byte allowance.

SSE framing follows the [HTML event-stream interpretation
rules](https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation):
bytes are retained through complete events, data fields are joined with newlines,
and LF, CR, CRLF, comments and a leading UTF-8 BOM are supported. A trailing event
without its blank-line terminator is discarded. Empty priming events allowed by
the [Streamable HTTP transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
are skipped. Other MCP data events must contain valid JSON; malformed JSON reports
a transport error. This request/response transport does not automatically
reconnect or replay SSE event IDs.

Regression fixtures exercise Unicode at every network byte boundary, multiline
events, chunked local HTTP, exact-limit output, incomplete lines, catalog limits,
stderr overflow and cancellation. They do not contact a provider, use credentials
or execute a real Skill.
