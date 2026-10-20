# Autonomous usage budgets

OrbitPal has one installation-owned daily budget shared by all bots. Configure it on the **Autopilot** page in Studio. This is separate from each bot's rolling hourly **contribution-attempt** limit: one contribution attempt may make several dialog calls and then synthesize speech and store a memory. Local walking, idle animation, gestures and Stop require no dialog budget.

The default daily limits are 100 dialog dispatches, 20,000 spoken Unicode characters, 200 Hindsight requests, 100 image attachments and 50 MiB of decoded image data. Hard token limiting is off initially. A zero component limit blocks new consumption of that component. Limits are editable explicitly in Studio; saving them, switching modes, selecting or copying bots, importing profiles and restarting the app never reset usage. Settings and usage are private installation data and are omitted from bot exports, public overlay events and Control-MCP summaries.

## What counts

| Component | Admission and counting rule |
| --- | --- |
| Dialog | Every autonomous model dispatch, including every tool-follow-up round, is admitted separately. A cached conversation or one contribution ticket does not exempt later calls. |
| TTS | Each admitted native HTTP synthesis attempt consumes the exact number of Unicode characters in the submitted spoken text, including ElevenLabs audio tags. Display text is not the counting source. An existing audio-cache hit does not synthesize again. Browser/system speech does not call a metered HTTP provider. |
| Hindsight | Recall and post-playback retain each consume one request, shown separately. Retain inherits the original job's trusted origin and bot, even when delayed until playback. A skipped/disabled recall or retain consumes nothing. |
| Images | Count every attached image and its decoded byte length on each outgoing dialog round. A fresh screen frame and any retained MCP image attachments are counted when actually submitted. Merely capturing an image does not transmit it. Image caps do not block a zero-image dialog, TTS or Hindsight when an image limit is lowered below earlier spend. |

Only native `Autopilot` and `AutomaticChat` origins are autonomous. Automatic viewer replies use a dedicated native command, require that bot's current auto-reply setting and are checked again before each provider round. A conversational `source`, prompt, skill, model response or MCP argument cannot change the origin. A manual Studio send, manual queue selection, microphone turn, voice preview, model discovery and user-started transcription remain operator actions outside these autonomous limits. This preserves manual use and Stop when the autonomous ledger is unavailable; it is **not an installation-wide bill cap**. Generic user-installed tools/services and their own internal purchases or model calls are outside this host-provider ledger. Their existing permissions still apply.

Manual model/TTS/STT commands and budget settings require the main Studio window. The built-in Control-MCP server exposes only its existing bounded local controls; it cannot start a dialog, synthesize speech, edit limits or relabel a job as manual. Remote provider bodies, credential identifiers, image data, prompts and arbitrary diagnostics are never stored in the budget ledger or denial messages.

## Dialog counting modes

- **OrbitPal calls / CLI jobs (`dispatch`, default):** one native direct HTTP dialog dispatch or one coding-CLI job consumes one slot. Codex and Claude Code may perform multiple internal model requests, tool rounds or retries inside one CLI job. OrbitPal cannot count or price those opaque internal operations. This mode is a bound on OrbitPal dispatches/jobs, not a bound on the CLI's provider calls.
- **Direct HTTP requests (`direct`):** autonomous coding-CLI jobs are rejected before retrieval, screen capture or model dispatch. Every supported host HTTP dialog send to the configured endpoint is reserved separately. Automatic redirects and client protocol retries are disabled. The remote endpoint may itself proxy, retry or use additional models: its internal work and invoice are unknown and outside this guarantee.

If the current UTC day already contains a CLI job, switching to direct mode blocks further autonomous dialogs with `request_history_unbounded` until the next UTC day. Returning to dispatch mode does not erase that history. Current CLI jobs in direct mode are rejected as `request_accounting_unsupported`. Ancillary limits continue to apply separately.

## Optional conservative hard token limit

A hard daily token limit is available only when OrbitPal can reserve a verified conservative upper bound for the complete outbound request. Unsupported integrations are rejected; they are never silently treated as zero tokens. At present the audited capabilities are exact `gpt-4o-2024-08-06` and `gpt-4o-mini-2024-07-18` snapshots on `https://api.openai.com/v1/chat/completions` over the default TLS port. This is a capability allowlist, not a recommendation to choose an older or deprecated snapshot. Aliases, newer or unknown models, nonmatching endpoints, custom proxies, Anthropic, local servers and coding CLIs have no asserted hard token bound.

For these snapshots, OrbitPal reserves the documented 128,000-token whole context window **plus** the enforced `max_completion_tokens` cap (32–8,192). This deliberately coarse reservation includes context/framing/images and overcounts output conservatively. The hard limit must be large enough for that reservation; the UI's initial optional draft is 1,000,000 tokens. This is not a tokenizer estimate and there is no claimed euro/dollar ceiling.

OpenAI-compatible usage receipts are accepted only with or after a terminal choice, when integer nonnegative prompt/completion totals add up exactly and remain bounded. Running/partial usage before completion cannot release a reservation. A valid receipt can replace that call's conservative reservation with observed usage. Missing, malformed or interrupted-stream receipts leave the full upper reservation charged and the usage unknown. A completed response waits at most two seconds for requested trailing usage; integrations that were not asked for a usage trailer keep their immediate finish behavior. Integrations without a verified upper continue to count requests in request-only mode, with unknown tokens visible. Self-reported receipts from an unbounded integration do not retroactively establish a hard bound.

If any earlier dialog of the current UTC day lacked a verified token upper bound, enabling a hard token limit blocks further autonomous dialogs with `token_history_unbounded` until the next UTC day, including after restart. Explicitly returning to request-only mode remains possible; it does not erase spend. If a provider reports more tokens than its verified bound, the receipt is recorded and further autonomous consumption fails closed for that day.

Capability sources checked on 2026-10-20:

- [OpenAI conversation state / context windows](https://developers.openai.com/api/docs/guides/conversation-state)
- [GPT-4o mini model snapshots and context](https://developers.openai.com/api/docs/models/gpt-4o-mini)
- [Chat Completions: output bounds and streaming usage](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)

## Atomic admission, cancellation and UTC days

The SQLite core uses `BEGIN IMMEDIATE` to check all bots' spend and write a durable **possibly-sent** reservation in one transaction. The production transport future cannot be polled until that transaction commits. Two bot connections cannot both spend the same remaining slot. Every follow-up is admitted again. Cheap nonspending checks run before blocking credential access and optional retrieval/capture; the authoritative atomic check runs again after required credential access immediately before polling the transport. A cheap check does not authorize a later send.

A UTC bucket is the day of **native admission**, not the remote invoice timestamp or packet-arrival time. Ordinary network scheduling can straddle midnight. Delayed post-playback retain obtains its own admission on the current day; it does not reuse the model's old reservation. A response settling after midnight updates its original admission bucket. The persisted day high-water mark prevents a backward local clock from resetting spend; a forward UTC day opens a new bucket. This uses the host clock and does not claim protection against an administrator rewriting the clock or database. No daily rollover starts an autonomous session.

Once an operation may have been sent, HTTP errors, timeouts, cancellation and dropped futures retain its request/ancillary charge. No automatic refund or retry occurs. Pending token reservations remain fully charged. Restart converts pending entries to unknown; it does not replay work, revive sessions or authorize a late receipt to reduce an already-unknown charge. A cancellation detected before admission does not reserve; cancellation after admission conservatively keeps the charge. Local movement and Stop remain available after a denial.

Usage shows overall and per-bot totals, observed tokens, charged tokens, unknown token requests and pending operations. **Charged tokens already include reserved tokens**; reserved is a subset, not another amount to add. Ancillary units are attempts/characters/attachments, not provider-internal billable duration or invoice units. The most recent denial has a fixed public reason without provider/private payloads.

## Storage and recovery

The versioned metadata, validated limits and reservation ledger are installed in the existing `cohost.sqlite3`. Bounds include 50,000 retained operations and 1,024 bot identities per day; rows more than 31 days old are pruned when new work is admitted. Exhausting capacity denies new autonomous work. Invalid settings or optional accounting data never prevent normal app/profile startup. Missing/unsupported metadata, malformed/current ledger entries, invalid/future bucket keys and incomplete layouts fail autonomous work closed.

If usage cannot be read, Studio labels it **unknown** and offers an explicit collapsed settings-only repair form. Its defaults are an unconfirmed proposal, not zero usage. The operator can replace malformed settings only when the schema is supported. Saving never deletes the ledger; a corrupt ledger can keep the snapshot unavailable and autonomous work blocked. No automatic retry/reset repair is performed. Backups and administrator-level database repair remain separate from these controls.

## Validation scope

The pure core uses actual SQLite fixtures with controlled days, competing real connections, multiple mocked tool rounds, failures, restart and settings-mode/history switches. Native tests exercise the actual Store and production metered-future seam, including cancellation before a mock transport is polled. Localhost HTTP fixtures exercise real TTS and Hindsight adapters without accounts. Token capability, receipt validation, image retransmission cost and Control-MCP rejection are tested independently. These tests do not establish provider invoices, native Keychain dialogs, real CLI internals, screen permissions or end-to-end paid model execution.
