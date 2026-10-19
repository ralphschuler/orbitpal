# Streamer AI Cohost → OrbitPal

Version 0.8.0 introduces the name **OrbitPal**, the repository `ralphschuler/orbitpal`, the executable `orbitpal` and `OrbitPal-vVERSION-PLATFORM` downloads. Existing released files and version tags keep their original names. The MIT license is unchanged; earlier copyright notices are retained.

See the [rename audit inventory](BRANDING-AUDIT.md) for current product names and deliberately retained compatibility identifiers.

## Existing bots and credentials

The display name changes without creating another app data store. The bundle identifier remains `studio.streamer-ai.cohost`; the existing SQLite location, keychain service `streamer-ai-cohost`, profile package identifier, `cohost://` events and UI-language storage key stay unchanged. Installed MCP/Skill IDs, grants and Hindsight tags are unchanged. New copied external MCP configurations use the display key `orbitpal`; previously configured local clients still connect to the same authenticated endpoint after their session token is refreshed normally.

Do not delete old app data or reset OS permissions to perform this migration. macOS screen/microphone permissions may need renewed confirmation because release apps remain ad-hoc signed. Use the [microphone recovery instructions](MICROPHONE-PERMISSIONS.md) if needed.

## Updates across the repository rename

New clients check the canonical endpoint:

`https://github.com/ralphschuler/orbitpal/releases/latest/download/latest.json`

Installed 0.6/0.7 clients check the former repository URL and permit downloads only when the initial asset URL has that exact old repository path. GitHub redirects the old repository slug after a rename, but an old client rejects a new canonical URL before downloading. Therefore the release manifest intentionally keeps **only** the fixed former asset prefix:

`https://github.com/ralphschuler/streamer-ai-cohost/releases/download/vVERSION/OrbitPal-vVERSION-…`

The release workflow must run from the official `ralphschuler/orbitpal` repository. There is no configurable asset repository. New clients allow exactly the canonical and former paths under the same owner; other owners/repositories, nested paths, queries, fragments, insecure hosts and wrong release tags remain rejected. The updater public key and mandatory signed-version binding are unchanged.

**Keep the former repository redirect intact. Do not create or transfer a new repository at `ralphschuler/streamer-ai-cohost`.** Breaking that redirect prevents installed old clients from checking/downloading an update, even though signature checks still prevent an unsigned replacement from being installed.

## Platform installation identity

- **macOS:** in-app updates replace the existing `.app` contents at the existing path. Its outer directory can remain `Streamer AI Cohost.app`, while its display name, bundle metadata and executable are OrbitPal. A new manual install is named `OrbitPal.app`. Avoid keeping and opening two copies of the same bundle. The installed updater's restart logic re-reads the bundle metadata, so a changed executable name is supported.
- **Windows MSI:** the former x64 UpgradeCode, `A9976BDA-0F16-5FD9-A9C4-BDD0372B8376`, is pinned explicitly. A major upgrade replaces the former MSI product rather than adding another installed application. New installation/shortcut labels use OrbitPal. The former Tauri MSI intentionally skips `RemoveShortcuts` during a major upgrade. A first-party WiX fragment therefore removes its three exact legacy desktop, Start-menu and uninstall link names only when an upgrade is detected and the former MSI's `InstallDir` and corresponding shortcut-ownership registry markers were found. It uses ordinary MSI `RemoveFile`/`RemoveFolder` tables, no script, custom process, wildcard or recursive deletion. The former directories are removed only when empty. Pins and user-created shortcuts may need manual recreation.
- **Windows NSIS:** a narrowly changed copy of the locked Tauri CLI 2.12.3 template preserves the former uninstall and install-directory registry keys. The displayed product becomes OrbitPal. Existing known desktop/Start-menu shortcuts are retargeted only when they point to the exact old executable; an OrbitPal shortcut is added. Old shortcut names may remain to preserve existing pins. Uninstall removes these retained links only when they target the app's executable. User-created links and taskbar pins may still need manual recreation.
- **Linux AppImage:** in-app updates preserve the existing AppImage file path; downloaded new packages use OrbitPal names. Debian packages are installed manually. The new `orbitpal` package declares `Provides`, `Conflicts` and `Replaces` for `streamer-ai-cohost`, so a package-manager installation replaces the former package rather than keeping duplicate launchers. User app data stays at the unchanged bundle-ID location. No package-manager changes run from the app.

The Windows publisher default remains **`streamer-ai`**, derived by the locked Tauri CLI from the second component of `studio.streamer-ai.cohost` when `bundle.publisher` is absent. Cargo authors do not determine this value. The legacy install-directory key is therefore **`Software\streamer-ai\Streamer AI Cohost`**; its default value belongs to NSIS and its `InstallDir` value/shortcut markers belong to MSI. Packaging checks require this exact publisher identity, so changing `bundle.publisher` or the identifier cannot silently break migration.

## Migration validation

The required Windows CI job builds new unsigned installer fixtures without release signing secrets. On the disposable GitHub-hosted runner it downloads both official 0.7.0 installers at fixed URLs, verifies their pinned SHA-256 sums before executing them, and upgrades each installer type separately. It checks one product registration, the new executable and shortcuts, and preservation of an app-data sentinel. For MSI it first resolves the former product's actual install location and shortcut targets, then requires all three formerly owned legacy links to be gone after the upgrade. No application executable is launched. NSIS is uninstalled before the MSI case; this does not request app-data deletion.

These fixture installs validate packaging migration, not live credentials, microphones, screen capture or external services. CI and native bundle verification results must be checked for the final commit; source changes alone do not establish a successful upgrade.

When updating the locked Tauri CLI, review `windows/installer.nsi` against its recorded upstream source, preserve the attribution and rerun the installer migration smoke. Do not regenerate the default MSI UpgradeCode from the new name.
