# Security policy

Security fixes target the latest released version and `main`. Older releases do not have a separate maintenance commitment.

## Report privately

Use [GitHub private vulnerability reporting](https://github.com/ralphschuler/streamer-ai-cohost/security/advisories/new). If unavailable, contact the repository owner through an available private GitHub channel. Do not post exploitable details, tokens, private profiles or chat transcripts in public issues.

Include affected versions/platforms, reproduction steps using dummy data, impact and a suggested fix if available. We will assess the report as maintainers become available; there is no guaranteed response SLA.

## Trust boundaries

MCP servers and enabled skill scripts can run programs with the user's OS privileges. Only add trusted extensions. Viewer tool access is separately controlled by the host. Generated text and retrieved sources cannot grant new permissions.

Secrets belong in the OS credential store. Dialog providers receive supplied context; voice providers receive their configured input. Hindsight is opt-in and may use further services according to its server configuration. OBS URLs are session-bound access links: do not share them publicly.

Initial prebuilt releases are unsigned/not notarized. Verify downloads against release SHA-256 checksums. Checksums provide integrity, not an independent publisher signature. No automatic in-app updater is enabled.

## Carried dependency correction

The Linux GTK stack requires GLib 0.18. We vendor 0.18.5 with the original upstream fix for RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g; optimized iterator regression tests and source hashes run in Linux CI and releases. See [the backport record](vendor/glib/COHOST-BACKPORT.md). Version-only scanners may still identify 0.18.5; the source correction is documented rather than concealed with a different version.
