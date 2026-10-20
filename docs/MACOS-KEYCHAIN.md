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

## Avoiding unnecessary requests

Dialog and model-catalog operations for **Codex and Claude Code do not read an
OrbitPal model API key**, even when an older saved configuration still references
one. Those integrations manage authentication in their own CLI. Native requests
for the browser/system voice also resolve no API key; browser playback and the
existing unsupported-native-operation error remain unchanged.

Actual HTTP integrations still retrieve their configured credentials. In
particular, a local/compatible server may require authentication: its loopback
address does not make a configured token unnecessary. Model, voice, STT, Hindsight,
MCP and platform-route permissions remain independent. No session cache, permanent
authorization override or new credential storage format is introduced by this
change. It removes proven unused reads; it does not claim to explain or eliminate
every user's native prompt.

Local regression tests use synthetic credentials and the in-memory mock store.
They check that skipped operations never consume a registered credential's injected
read error, while HTTP consumers retain their exact reference and errors. They do
not inspect real keys, modify ACLs or establish native prompt/lock behavior.
