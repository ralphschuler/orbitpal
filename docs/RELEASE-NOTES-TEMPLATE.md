## New in 0.11.0

- A clearer Studio: readable controls, consistent dark colors, one save/status area, compact crew controls and the composer directly below your character. Text navigation adapts to smaller windows.
- A more understandable Skills & MCP library: shared installations and the selected bot’s individual grants are clearly separated. Technical details expand when needed; permissions, cloud transmission, costs and errors remain visible.
- Safer editing: changed library drafts ask before discard, knowledge/history deletion requires confirmation, and pending saves cannot spill into a newly opened editor. Failed saves retain the draft.
- Better keyboard and focus behavior: Escape closes foreground help or the bot picker before an editor or output, confirmations block background shortcuts, and the narrow-window bot picker remains clickable. German, English, French and Spanish remain supported without translating bot content.

## Downloads and updates

Windows x64 includes NSIS and MSI installers. macOS includes Apple Silicon and Intel ZIPs/DMGs. Linux x64 includes AppImage and DEB packages. The pipeline requires platform checks, build-provenance verification and cryptographic verification of all five updater payloads before publication. Dependency inventories, source code and SHA256SUMS are included.

In-app updates require explicit confirmation. Linux in-app installation uses AppImage; DEB packages update manually. Existing profiles, bot grants, credential IDs, installed app identity and updater trust key stay compatible.

macOS retains the pinned persistent self-signed identity and is **not Apple-notarized**. Windows installers have **no Authenticode signature**. Real installed-update and microphone/screen permission retention remain unverified; updating from ad-hoc releases through 0.9.0 can require renewed grants. This UI release keeps the existing signing identity.

The UI was checked with synthetic browser fixtures across four languages and three sizes, including draft, deletion, keyboard, deferred-save and transparent-overlay cases. These checks do not establish native installation, assistive-technology or live-provider acceptance. Wayland remains experimental; unfinished desktop acceptance stays in draft PR #56 and issue #26. Signing/recovery/installed-update acceptance (#17) and remaining upstream dependency migrations (#19) also remain open.

See the [changelog](https://github.com/ralphschuler/orbitpal/blob/v0.11.0/CHANGELOG.md), [Studio UI acceptance checks](https://github.com/ralphschuler/orbitpal/blob/v0.11.0/docs/STUDIO-UI.md), [Wayland validation](https://github.com/ralphschuler/orbitpal/blob/v0.11.0/docs/WAYLAND-VALIDATION.md) and [release trust](https://github.com/ralphschuler/orbitpal/blob/v0.11.0/docs/RELEASE-TRUST.md).
