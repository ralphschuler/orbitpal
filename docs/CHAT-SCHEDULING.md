# Automatic chat scheduling

Automatic Twitch/YouTube replies begin only after the operator starts the Cohost.
The selected Studio figure and bots with active character windows or enabled OBS
outputs can participate. Each bot still needs a configured dialog model (Codex and
Claude Code may use their default), an automatic chat mode and a matching,
unexpired message that this bot has not already answered. Manual bots and inactive
outputs do not reserve a turn.

Messages matching a bot's configured **Ansprache / Auslöser** trigger have priority
over catch-all replies. Matching remains case-insensitive substring matching.
Platform notifications retain the existing trigger bypass; that alone gives them
no direct-address priority. Within either priority class, the least recently
served eligible bot goes next. Initial ties follow the crew order; the oldest
eligible message in the chosen class is used for that bot.

With three continuously eligible bots in the same priority class, each gets a
slot within three completed automatic turns even when matching chat continuously
arrives for the first bot. A newly active bot is eligible for the next selection.
This guarantee does not override repeated manual/operator requests or a continuous
stream of higher-priority direct addresses. Chat still takes precedence over
spontaneous Autopilot speech.

One exact turn claim covers asynchronous permission preflight, model work and
playback. Repeated scheduler polls cannot start a duplicate while any of these
stages is pending. Manual selections and operator messages preempt automatic work
that has not started, then reserve their own claim through preflight and playback.
They respect the existing busy/audio guards; an already playing reply remains
serialized. A failed or cancelled attempt releases its claim. Before a queued
request is dispatched, its bot eligibility, expiry, configuration, queue membership
and current claim are checked again. Once accepted for dispatch, the message is
marked answered for that bot, including when a later provider/audio error occurs;
the operator can explicitly select **Erneut mit …** to retry.

Stop invalidates pending claims and clears the queue. Pausing automatic chat
invalidates pending automatic preflight without interrupting an accepted reply or
an operator request. Late completion cannot release a newer claim or revive a
cancelled request. Bot changes, output removal, updater installation and app
shutdown retain the existing cancellation and presentation rules.

The visible queue retains its existing maximum of 200 messages and per-bot expiry
filters. Duplicate native events with the same platform, reply route and message
ID preserve their existing answered state. Different platform routes remain
separate, and dispatch passes the original route unchanged to the native host,
which continues to validate connection ownership and optional platform posting.
The scheduler grants no new platform, screenshot, model-tool or audio permissions.

Regression fixtures continuously refill a queue and inspect the actual selected
bot IDs, including new joiners, inactive/manual bots, direct addresses, route
identity, deferred manual/automatic preflight, provider failures, Stop and stale
completion. They use synthetic messages and playback promises, with no live chat
accounts, model calls or recordings.
