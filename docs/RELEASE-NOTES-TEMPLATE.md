## New in 0.14.0

- Choose a saved energy render profile: at most 20 FPS, pixel ratio 1 and no dynamic shadows. Hidden views avoid unnecessary GPU submissions while native Life and Stop keep running independently.
- Select a second character explicitly for a play invitation. Studio and the native menu recheck the same social coordinator; missing or stale selections cannot become a solo rewarded activity.
- Use the typed native Pet action when the current character has an explicitly allowed, uniquely resolved authored Pet clip. Successful frames and native acknowledgements govern completion; missing capabilities stay unavailable.
- Read daily recaps in the local calendar, with confirmed hobby/goal milestones and access to all retained album facts and social memories. Private entries still require explicit sharing before public preview.
- Prepare an empty private system-bus transport for the isolated GNOME renderer harness, preserving its scope and absent host services.
- Reconcile the existing implementation against all 18 open issues and record dependency order and remaining acceptance. There were no open pull requests at the initial inventory.

## Known limits

Desktop Life remains experimental. Actual multi-character movement, paired scenes, hidden operation, sleep/reconnect, monitor/DPI and OBS acceptance remain open under #72/#75 and related issues. No measured 1/3/5-character hardware performance baseline is claimed (#66). The prior native macOS fixture and isolated Linux renderer smoke did not pass. The new private-bus preparation alone is not a successful renderer measurement.

Full native interaction/menu and authored-Pet visual acceptance remain open (#81). The five unmaintained transitive dependency families remain tracked in #19. Wayland screen context remains experimental pending complete application acceptance (#26). See the issue-by-issue backlog reconciliation for exact remaining work; merging this PR does not close unproved acceptance criteria.

## Downloads and updates

Windows x64 includes NSIS and MSI installers. macOS includes Apple Silicon and Intel ZIPs/DMGs. Linux x64 includes AppImage and DEB packages. Publication requires all platform checks, tagged build provenance and cryptographic verification of the updater payloads. Source, dependency inventories and SHA256SUMS are included.

In-app updates require explicit confirmation. Linux in-app installation uses AppImage; update DEB packages manually. The existing installed identity, credential namespace and updater trust key are retained.

macOS uses the existing persistent self-signed identity and is not Apple-notarized. Windows installers have no Authenticode signature. Real installed-update permission retention remains unverified. MIT and third-party licenses are unchanged.

See the [changelog](https://github.com/ralphschuler/orbitpal/blob/v0.14.0/CHANGELOG.md), [backlog reconciliation](https://github.com/ralphschuler/orbitpal/blob/v0.14.0/docs/BACKLOG-2026-10-21.md), [render profiles](https://github.com/ralphschuler/orbitpal/blob/v0.14.0/docs/RENDER-PROFILES.md), [dependency tracking](https://github.com/ralphschuler/orbitpal/blob/v0.14.0/docs/DEPENDENCY-MAINTENANCE.md) and [release trust](https://github.com/ralphschuler/orbitpal/blob/v0.14.0/docs/RELEASE-TRUST.md).
