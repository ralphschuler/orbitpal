# macOS permissions and update identity

OrbitPal checks microphone and native screen-recording permission before an explicitly started feature uses them. Checking a status does not open a device, capture pixels or transmit data. OS permission remains separate from each bot's image grant, model destination and screen-context setting.

## Granting access

Start recording/listening, enable screen context, or start a conversation/automation that uses screen context. If access is missing, the Studio offers a permission request, a link to the relevant System Settings page, another status check and cancellation. Continue only after access is available. Returning from Settings refreshes the status; it does not start a microphone or replay a pending automatic conversation.

For first use, macOS can show its native request. After an earlier denial macOS may not show it again. Use **System Settings → Privacy & Security → Microphone** or **Screen Recording / Screen & System Audio Recording**, enable OrbitPal, return to the app and check again. A complete quit/relaunch may be necessary. Restricted device policies need the device administrator; retrying a prompt cannot override them.

Stop, cancellation, a changed bot/configuration and an application update invalidate pending starts. Background model/tool calls and automatic retries cannot ask the OS for new permission. When an automatic conversation lacks screen permission, use the Studio's permission action and then explicitly restart the feature. Other file access uses the explicit native file picker; permissions needed by an external MCP process belong to that process.

## Why an update can ask again

The fixed bundle and signing identifier is `studio.streamer-ai.cohost`, the product is `OrbitPal`, and the executable is `orbitpal`. Keep these values across releases. They also preserve existing data locations; changing identifiers is not a permission repair.

Public packages through **v0.9.0** use **ad-hoc signing**. Apple explains that TCC (the privacy subsystem) tracks the app's *designated requirement*. An ad-hoc requirement identifies a particular binary by its code hash, which changes with a new build. A fixed display name or updater signature alone cannot preserve that identity.

Published releases from **v0.10.0** use a **persistent self-signed codesigning certificate**, at no Apple-account cost and without Apple notarization. The release pipeline requires the same certificate for Apple Silicon and Intel, verifies its reviewed complete public SHA-256 fingerprint and enforces an exact app-ID/certificate-bound designated requirement. Its public certificate/pin is versioned and the matching encrypted archive/password are protected repository secrets. Missing keys or signing failures never fall back to ad-hoc output.

The distinction between the available identities remains:

| Identity | Update continuity | Distribution trust |
| --- | --- | --- |
| A long-lived self-signed codesigning certificate/key | Can provide a stable, certificate-bound requirement; OrbitPal still needs real A→B update/TCC acceptance on supported macOS versions | No Apple-verified identity or notarization; downloaded-app Gatekeeper approval remains separate |
| Maintainer-owned Developer ID Application certificate with consistent app/team identity | Apple's supported identity for distribution outside the App Store; still requires update acceptance | Pair with successful notarization and stapling |

The persistent identity has shipped for both architectures since v0.10.0; see the actual pipeline evidence in [release trust](RELEASE-TRUST.md). A self-signed certificate is technically distinct from ad-hoc signing; an Apple Developer account is not required merely to create a persistent signing identity. Apple recommends Developer ID for public distribution. Our own certificate does not provide Apple-verified publisher trust or notarization, and downloaded-app first-launch approval remains separate. We do not install a trusted root certificate or change Gatekeeper settings on the user's Mac.

macOS CI uses two harmless app fixtures and temporary keys to verify code-identity continuity, another key's rejection even with the same certificate name, and tamper rejection. The fixtures are signed but never executed and never request privacy access. **Actual microphone/screen TCC retention across two OrbitPal versions still requires manual acceptance**; passing those signatures alone is not evidence of preserved OS grants.

Whichever path is chosen, preserve and protect the signing key, pin the certificate or verified Apple team in the release policy, use the same identity for both architectures and keep its designated requirement cryptographically bound. A certificate's display name or the bundle identifier alone is not an identity proof. Losing/changing the identity can require another permission grant.

Moving from existing hash-bound ad-hoc builds can require one final approval. There is no legitimate way for OrbitPal to silently copy those permissions to a different signer. OS policy changes, revocation or administrator restrictions can also require new approval later. Never edit/reset the user's TCC database or weaken the signing requirement to avoid consent.

The Tauri updater key remains a separate trust mechanism and must not change as part of OS signing. See [release instructions](RELEASING.md) for offline identity preparation, CI cleanup and protected provisioning. Artifact provenance is already verified by the current release workflow; Apple-verified signing/notarization and remaining operational/native acceptance in #17 remain separate.

## Credential access is separate

API-key dialogs belong to the macOS Keychain rather than TCC. Saved values and
per-item application authorization have separate lifetimes. See [macOS Keychain
access](MACOS-KEYCHAIN.md) for Allow versus Always Allow, multiple credential items,
locked Keychains and the operation-specific handling of CLI/system-voice keys.

References: [Apple TN3127](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements), [Apple Code Signing Guide](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/AboutCS/AboutCS.html), [Apple screen request API](https://developer.apple.com/documentation/coregraphics/cgrequestscreencaptureaccess()), [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/).
