# macOS Keychain access

OrbitPal stores API keys and platform tokens in the operating system credential
store. On macOS this is the file-based User/login Keychain, using generic-password
items. The existing service name `streamer-ai-cohost` is intentionally retained;
the configured credential reference identifies the item's account. Profiles and
exports contain references, never the stored values. Reopening or updating the
app does not require entering the API key again merely to save it.

Saving a value and allowing an application to retrieve it are separate decisions.
Apple's [Keychain access guide](https://support.apple.com/en-gb/guide/mac-help/kychn002/26/mac/26)
distinguishes **Allow / Allow Once**, which permits the current retrieval, from
**Always Allow**, which remembers the application for that item. The decision is
per item: model, voice, Hindsight and platform credentials can require separate
grants even when the dialogs show the same service label. The app's help icon
beside credential fields explains this in all four interface languages.

An already saved key therefore may still cause a native authorization dialog.
A locked login keychain or a mismatch with the login password can separately ask
for the keychain password; see [Apple's explanation](https://support.apple.com/en-lamr/guide/keychain-access/-kyca1242/mac).
These dialogs are distinct from microphone and screen-recording permissions in
System Settings. Granting one does not grant another. Nothing in OrbitPal changes
Keychain lock settings, grants every application access, resets the Keychain or
copies TCC privacy records.

## App identity across updates

Published releases through **v0.9.0** were ad-hoc signed. **v0.10.0 and later** use
the pinned persistent self-signed identity, the unchanged identifier
`studio.streamer-ai.cohost` and an exact certificate-bound designated requirement.
The [public policy](../release/macos-signing-policy.json) and
[release trust documentation](RELEASE-TRUST.md) describe the verified release
pipeline and remaining native installation tests. This is not Apple notarization.

Apple documents that the file-based Keychain tracks the application's designated
requirement and can recognize updates signed with the same persistent identity,
including self-signed identities. See [TN2206](https://developer.apple.com/library/archive/technotes/tn2206/)
and [Keychain access control lists](https://developer.apple.com/documentation/security/access-control-lists).
An item created by an older ad-hoc build may still need the operator to authorize
the current app. Different installed copies, an older item's ACL, a one-time grant
and a locked Keychain are possible explanations; repeated dialogs alone do not
prove which applies. Keep the pinned signing identity and original credential
namespace rather than deleting/recreating items as a routine repair.
If dialogs recur after **Always Allow**, selecting it again is not a verified
repair. The app/item/lock cause still needs separate diagnosis; session reuse
reduces subsequent OS reads without claiming to repair that underlying policy.

## Avoiding unnecessary requests

Dialog and model-catalog operations for **Codex and Claude Code do not read an
OrbitPal model API key**, even when an older saved configuration still references
one. Those integrations manage authentication in their own CLI. Native requests
for the browser/system voice also resolve no API key; browser playback and the
existing unsupported-native-operation error remain unchanged.

Actual HTTP integrations still retrieve their configured credentials. In
particular, a local/compatible server may require authentication: its loopback
address does not make a configured token unnecessary. Model, voice, STT, Hindsight,
MCP and platform-route permissions remain independent. Removing unused reads does
not explain every native prompt. The separate session option below applies only
when the operator explicitly enables it.

## Optional reuse during an OrbitPal session

Under **API connections**, macOS users can enable **Reuse approved keys during this
session**. It is **off by default**. The preference belongs to this installation,
applies to all bots, and is never copied or exported with a profile. Windows and
Linux retain their existing persistent-backend behavior and expose no session
setting. A new app process starts with an empty session, even if reuse is enabled.
Missing, malformed, non-text or oversized optional preference values default to
disabled without blocking app startup or including their payloads in diagnostics.

The first successful OS retrieval of a registered credential can be retained in
process memory. Later consumers of the exact same credential ID, including scoped
per-bot Stores, reuse that value. Missing entries and denied or failed reads are
never retained. Different credential IDs still require their own first retrieval;
this does not grant permanent access or change any item's ACL. Only successful
reads populate the session; saving a value does not silently approve a read.

**This is an application session policy, not an OS lock monitor.** If the macOS
Keychain is locked, an item is edited/deleted, or its access rights are revoked
externally after retrieval, its already loaded copy remains usable by OrbitPal
until the operator clears the session or the app exits. To observe external
changes, select **Lock keys now**, then **Approve keys again**. This limitation is
shown next to the switch before enabling it. Choose the default, disabled mode
when every retrieval must go through the OS policy. No platform notifications,
Keychain-status FFI, ACL rewriting or automatic OS unlock is used.

**Lock keys now** clears the broker's retained buffers, stops Studio's pending
responses and leaves future registered-key retrievals blocked until the operator
approves them again. Locking preserves the enabled preference. Restarting the app
starts a fresh session and can therefore prompt on the next OS retrieval. Turning
the option off also clears retained values and returns future reads to the OS.
An in-flight read completed after a lock, disable or new session is rejected and
cannot repopulate the session. Locking does not recall keys already handed to
HTTP requests, persistent stream/MCP connections or third-party subprocesses.
It cannot undo a request already sent. Disconnect existing connections or restart
OrbitPal when they must also discard their previously received credentials.

The broker retains at most **64 values, 1 MiB in total, and 64 KiB per value**.
Larger values or values beyond the retained-value budget are returned to their
request but are not cached; subsequent use can therefore prompt again. Persistent
values are never truncated or replaced. At most 64 different IDs may have pending
operations concurrently; excess operations fail with a bounded diagnostic. Each
ID's lane is released after its last operation/waiter. Independent IDs do not hold
one another's native dialogs behind a global cache lock.
References must retain the persistent-store ID grammar: 1–96 ASCII letters,
digits, hyphens or underscores. Corrupt registered references cannot allocate a
slot or reach the native credential backend.

Write/delete operations serialize with reads of the same ID and invalidate its
copy before and after mutation, including failures. Registered-ID checks remain
in front of cache hits. Clear/disable advances a session generation so a late
authorized read cannot return a stale value. Retained String buffers use
`zeroize` on removal; this does not promise to erase copies held by callers or
HTTP clients. No cached value appears in snapshots, logs, errors, profiles or disk.
Only the main Studio window exposes setting/lock commands; agent/MCP tools cannot
enable or resume this operator choice.

Local regression tests use synthetic credentials and the in-memory mock store.
They check that skipped operations never consume a registered credential's injected
read error, while HTTP consumers retain their exact reference and errors. They do
not inspect real keys, modify ACLs or establish native prompt/lock behavior. Pure
broker fixtures cover single-flight success, independent IDs, delayed reads versus
lock/disable, mutation invalidation, bounds and slot cleanup. Store fixtures check
shared scoped ownership, registry gating, mutation behavior and installation-only
preference persistence. Native acceptance must still verify first-use prompts,
Allow Once/Always Allow, app restart and the documented external-lock boundary.
