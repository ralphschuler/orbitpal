# Security policy

Security fixes target the latest released version and `main`. Older releases do not have a separate maintenance commitment.

## Report privately

Use [GitHub private vulnerability reporting](https://github.com/ralphschuler/orbitpal/security/advisories/new). If unavailable, contact the repository owner through an available private GitHub channel. Do not post exploitable details, tokens, private profiles or chat transcripts in public issues.

Include affected versions/platforms, reproduction steps using dummy data, impact and a suggested fix if available. We will assess the report as maintainers become available; there is no guaranteed response SLA.

## Trust boundaries

MCP servers and enabled skill scripts can run programs with the user's OS privileges. Only add trusted extensions. Viewer tool access is separately controlled by the host. Generated text and retrieved sources cannot grant new permissions.

Secrets belong in the OS credential store. Dialog providers receive supplied context; voice providers receive their configured input. Hindsight is opt-in and may use further services according to its server configuration. OBS URLs are session-bound access links: do not share them publicly.

Windows packages lack Authenticode signatures; macOS packages are ad-hoc signed without notarization. Verify manual downloads against release SHA-256 checksums. Checksums provide integrity, not an independent publisher signature.

From 0.6.0, automatic checks use this repository's fixed HTTPS stable-release endpoint. Download and installation require explicit studio confirmation. Native validation requires a newer stable version, a same-version repository asset and a Tauri signature verified against the embedded public key, including signed version binding. Updates are unavailable to avatars, MCP tools and skills. The private updater key belongs only in protected release infrastructure and an offline maintainer backup, never the repository, profiles or public artifacts. See [updates](docs/UPDATING.md) and [release operations](docs/RELEASING.md).

## Carried dependency correction

The Linux GTK stack requires GLib 0.18. We vendor 0.18.5 with the original upstream fix for RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g; optimized iterator regression tests and source hashes run in Linux CI and releases. See [the backport record](vendor/glib/COHOST-BACKPORT.md). Version-only scanners may still identify 0.18.5; the source correction is documented rather than concealed with a different version.

The updater enables only TLS and system-proxy support; unused legacy Tauri-v1 ZIP archive compatibility is disabled. Our release channel uses Tauri-v2 installer/AppImage/macOS archive formats. Do not restore the legacy feature or an obsolete direct ZIP dependency without reviewing its advisories. Dependency CI reports unmaintained transitive crates separately from vulnerability findings; see the [dated version/path matrix and upstream migration blockers](docs/DEPENDENCY-MAINTENANCE.md), tracked in issue #19. The full audit includes optional locked dependencies, even when the selected runtime features do not build them. No advisory is globally suppressed.
