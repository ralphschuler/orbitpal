# Rust dependency maintenance

This is the reproducible follow-up for [issue #19](https://github.com/ralphschuler/orbitpal/issues/19), reviewed on **2026-10-21** against the 0.12.0 source with the explicit native credential-store migration and Wayland portal dependencies. These entries are maintenance advisories. They are not evidence that an exploit was demonstrated in OrbitPal, and keeping this record does not fix the affected dependencies.

## Audit snapshot

`cargo-audit 0.22.2` fetched the RustSec database online before both full lockfile scans on 2026-10-21, after the supported Tauri 2.12.5 patch update. Both reports identify revision `4b587f00996ebe5c05a73eaef105ef241e876414`, updated `2026-10-21T08:20:38+02:00`, with 1,348 advisories. Both commands exited 0; no advisory IDs, architectures or operating systems were excluded.

| Lockfile | Packages | Vulnerability findings | Unmaintained warnings | Lockfile SHA-256 |
| --- | --- | --- | --- | --- |
| Production, `src-tauri/Cargo.lock` | 623 | 0 | 5 package/version warnings across the five IDs below | `eed53ff38e04472710620951323df3caa7bf54603f84cc7fc29bda1cbaf5651f` |
| Test-only credential compatibility reference, `scripts/fixtures/keyring-compat/Cargo.lock` | 167 | 0 | 3: `zbus_names 3.0.0`, `zvariant 4.2.0`, `zvariant_derive 4.2.0` | `75d931d23643091b7b93471f3daf6e10cea51e455f0088286feb1e7fffae091d` |

The production and reference JSON report SHA-256 values are `bd5102f202efb46dd5eac15ec404232aa36b41ed564748da282b602fd1ef8b72` and `4092fc76254a057f6ed52a867b113b7ceadfbebfbec4b455fd167b2a9894b02b`. Generated reports remain outside source control. The production lockfile still contains all five matrix entries and has no old keyring/zbus 4 path; that removed family remains visible in the intentionally historical reference lockfile. These results replace the earlier 624-package snapshot before the Tauri patch update; they do not establish selected-feature or platform compatibility by themselves.

This is a dated result, not a permanent clean bill of health. `cargo audit` scans the lockfile, which includes optional package dependencies. `cargo tree` with OrbitPal's selected features is a separate check of the active build graph; it must not be used to hide warnings from the full audit.

| Advisory | Exact locked versions | Selected build path | Supported migration and upstream status at review |
| --- | --- | --- | --- |
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html), `proc-macro-error` | `1.0.4` | Linux GTK: `OrbitPal → tauri 2.12.5 → gtk 0.18.2 → gtk3-macros 0.18.2 → proc-macro-error 1.0.4`; also `gtk → glib 0.18.5 → glib-macros 0.18.5 → proc-macro-error`. GTK/WebKitGTK also enter through the Wry/runtime graph. | New GLib generations removed this macro dependency ([gtk-rs-core #1288](https://github.com/gtk-rs/gtk-rs-core/pull/1288)), but Tauri 2.12.5 and WebKitGTK still require the 0.18 generation. Track [Tauri GTK4 #7335](https://github.com/tauri-apps/tauri/issues/7335) and [migration PR #14684](https://github.com/tauri-apps/tauri/pull/14684), both open. |
| [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html), `ttf-parser` | `0.25.1` | All desktop platforms: `OrbitPal → pdf-extract 0.12.1 → lopdf 0.42.0 → ttf-parser`. | `lopdf 0.45` replaced the parser with optional `skrifa` ([lopdf #556](https://github.com/J-F-Liu/lopdf/pull/556), merged). The released `pdf-extract 0.12.1` still requires `lopdf ^0.42`; [pdf-extract #156](https://github.com/jrmuizel/pdf-extract/issues/156) and [PR #163](https://github.com/jrmuizel/pdf-extract/pull/163) remain open. PR #163 includes an API adaptation and a proposed `pdf-extract 0.13.0` release. |
| [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html), `zbus_names` | `4.3.5` | `4.3.5`: Linux `OrbitPal → tauri-plugin-opener 2.7.2 → zbus 5.19.0 / zbus_macros 5.19.0`. The old optional keyring/zbus 4 family was removed by the migration below. | Names moved to `zbus::names` in zbus 6; see [zbus #2043](https://github.com/z-galaxy/zbus/issues/2043) (closed, migration released). Opener still requires `zbus ^5.9`; [plugins-workspace #3716](https://github.com/tauri-apps/plugins-workspace/issues/3716) tracks the open downstream migration. |
| [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html), `zvariant` | `5.15.1` | `5.15.1`: the same selected opener/zbus 5 graph, including `zbus_names 4.3.5`. The old optional keyring/zbus 4 family was removed by the migration below. | D-Bus types moved to `zbus::wire` in zbus 6; GVariant is a separate project. The same [opener migration #3716](https://github.com/tauri-apps/plugins-workspace/issues/3716) is required. An upgrade within zbus 5 retains this warning. |
| [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html), `zvariant_derive` | `5.15.1` | Linux: `OrbitPal → tauri-plugin-opener 2.7.2 → zbus 5.19.0 → zvariant 5.15.1 → zvariant_derive 5.15.1`. | Derives moved into zbus 6. The same downstream migration is required; do not substitute zbus 6 underneath APIs compiled against zbus 4/5. |

The [ttf-parser repository](https://github.com/harfbuzz/ttf-parser) now describes bug-fix-only maintenance, while the current RustSec entry still marks the published crate unmaintained. That changed upstream wording is not a reason to suppress the advisory or claim a new fixed release.

### Supported native credential-store migration

The previous 637-entry production lockfile included `keyring 3.6.3 → secret-service 4.0.0 → zbus 4.4.0 → zbus_names 3.0.0 / zvariant 4.2.0 / zvariant_derive 4.2.0`. That optional async backend was not enabled by OrbitPal, but the full lockfile audit correctly reported its three warnings. The migration replaces the monolithic keyring dependency through the published [keyring-core API](https://docs.rs/keyring-core/1.0.1/keyring_core/) and its explicit native store providers. Cargo re-resolved the lockfile normally; no package entries or advisory IDs were manually excluded.

- Common API: `keyring-core 1.0.1`, with each store constructed directly. No process-global default store is changed.
- macOS: [apple-native-keyring-store 1.0.2](https://docs.rs/apple-native-keyring-store/1.0.2/apple_native_keyring_store/) with `keychain`, preserving the file-based User/login Keychain, generic-password class, service and account. The protected-data backend is not enabled.
- Windows: [windows-native-keyring-store 1.1.0](https://docs.rs/windows-native-keyring-store/1.1.0/windows_native_keyring_store/) preserves the generic-credential target `user.service`, Enterprise persistence and UTF-16 little-endian password encoding.
- Linux: [dbus-secret-service-keyring-store 1.0.1](https://docs.rs/dbus-secret-service-keyring-store/1.0.1/dbus_secret_service_keyring_store/) with `crypto-rust`, retaining the synchronous persistent Secret Service backend and encrypted DBus transport. The explicit `target=default` modifier retains keyring 3's item attributes and default collection, including compatibility with app rollback. No volatile keyutils fallback is introduced.

The service remains exactly `streamer-ai-cohost` and existing credential IDs remain unchanged. Storage still consults its existing ID registry before reading; errors expose a fixed category instead of formatting backend payloads or malformed secret bytes. Ordinary unit tests use an in-memory store exclusively. The compatibility boundary is OrbitPal's existing keyring 3 entries; pre-OrbitPal keyring 1 entries lacking a `target` attribute are not migrated by this change.

The separate non-publishable `scripts/fixtures/keyring-compat/` manifest intentionally retains keyring 3 as a test reference. It is absent from the production manifest, lockfile, notices inventory and installers. Its own lockfile is audited explicitly by the advisory workflow without exclusions; its historical optional zbus 4 warnings are not fixed or hidden. The fixture uses the actual production credential module and the old API in one executable, running 15 operations across fresh processes: old-write/new-read, new-write/old-read, both update and delete directions, missing-entry handling, Unicode and process persistence. This avoids different macOS executable ACL identities between the two APIs; it is not a code-signing migration test.

Native execution is opt-in and restricted to disposable GitHub-hosted CI. macOS creates a private temporary keychain, saves and restores the previous default and complete search list in `finally` plus an `always()` recovery step. Linux starts an isolated HOME/XDG/DBus session and unlocked synthetic GNOME Keyring. Windows touches only one unpredictable UUID target in the disposable runner account and deletes it afterward. No fixture enumerates or reads developer credentials. See [the fixture instructions](../scripts/fixtures/keyring-compat/README.md) for validation status and reproduction boundaries.

### Separate Wayland portal graph

The direct `OrbitPal → zbus 6.0.0` and `pipewire 0.10.1` paths remain unchanged. They do not migrate opener's selected zbus 5.19.0 graph. The explicit credential-store migration removes zbus 4 and its three old package versions from the production lockfile, leaving the opener/zbus 5 advisory versions in the table above. Having zbus 6 installed does not by itself resolve those remaining families.

## Upgrade decisions

Fresh crates.io metadata on 2026-10-21 confirmed [Tauri 2.12.5](https://crates.io/crates/tauri/2.12.5), [tauri-plugin-opener 2.7.2](https://crates.io/crates/tauri-plugin-opener/2.7.2) and [pdf-extract 0.12.1](https://crates.io/crates/pdf-extract/0.12.1) as the latest published stable consumers. Their published requirements still select `gtk ^0.18`, `zbus ^5.9` and `lopdf ^0.42`, respectively. [GTK 0.19.0](https://crates.io/crates/gtk/0.19.0) instead uses `glib ^0.22` and is outside Tauri's compatible GTK requirement. [Lopdf 0.46.0](https://github.com/J-F-Liu/lopdf/releases/tag/v0.46.0) is released, but it is outside pdf-extract's `^0.42` requirement. A newer standalone binding, direct zbus 6 dependency or prerelease consumer does not migrate the supported runtime graph. The linked GTK4, opener and PDF consumer migrations remain open.

### Tauri 2.12.5 patch update

The regular `cargo update --manifest-path src-tauri/Cargo.toml -p tauri@2.12.4 --precise 2.12.5` update stays within the existing manifest constraints. The [upstream release](https://github.com/tauri-apps/tauri/releases/tag/tauri-v2.12.5) fixes Windows NSIS installer downgrade checks and an installer language-string name. It also selects `tauri-build`, `tauri-codegen` and `tauri-macros 2.7.4`, plus `tauri-runtime-wry 2.12.3`. Codegen now requires `ico 0.6` instead of `0.5`; its `png 0.18` requirement removes the otherwise unused `png 0.17.16` entry. The five Tauri crates retain their published Rust 1.90 minimum. Application versions, updater configuration, JavaScript dependencies and the GLib correction are unchanged.

Cargo also rebound 13 existing Windows dependency edges from the already locked `windows-sys 0.61.2` to the already locked `0.60.2`. No Windows package version was added or removed. Each published consumer manifest permits this result:

| Unchanged consumers | Published `windows-sys` requirement |
| --- | --- |
| `dirs-sys 0.5.0` | `>= 0.59.0` |
| `errno 0.3.14`, `rustix 1.1.5`, `tempfile 3.27.0` | `>=0.52, <0.62` |
| `muda 0.20.0`, `tray-icon 0.25.1`, `window-vibrancy 0.8.1` | `>=0.60, <=0.61` |
| `quinn-udp 0.5.16` | `>=0.52, <=0.61` |
| `rustls-platform-verifier 0.7.1` | `>=0.52.0, <0.62.0` |
| `socket2 0.6.5`, `uds_windows 1.2.1` | `>=0.60, <0.62` |
| `winapi-util 0.1.11` | `>=0.48.0, <=0.61.*` |
| `winreg 0.56.0` | `>=0.59, <=0.61` |

The lockfile was resolved by Cargo, without hand-editing dependency edges. These compatible ranges and the full audit do not replace the required final-head Windows, macOS and Linux CI.

Local validation of this patch on `aarch64-apple-darwin` passed locked fetch/metadata, all five selected reverse graphs, the full Cargo test command (636 unit tests), the renderer benchmark asset build and example's 11 protocol tests, manifest-version checks and all 46 release-packaging tests. Strict third-party-notice generation passed with no warnings; CI regenerates the bundled notices from its installed packages. All 122 GLib source-hash entries still match. The Linux-only optimized GLib regression and Windows installer behavior require their actual platform CI; the macOS test command runs zero cases from that Linux-only regression target. No app or GUI was launched for these checks.

The credential-store migration above uses released, explicit providers and removes three warned production package versions. The five remaining advisory families still require a supported GTK, opener or PDF consumer migration; no compatible published stable consumer update currently removes them.

Do not force GTK/GLib binding generations, patch transitive crates to incompatible majors, point production dependencies at unreviewed upstream branches, replace the persistent Linux credential backend with a volatile one, or delete optional lockfile entries by hand. These changes would either break supported behavior or make the audit record misleading.

The existing GLib source correction addresses the separate `RUSTSEC-2024-0429` defect. Keep the [backport provenance, exact source hashes and optimized iterator regression](../vendor/glib/COHOST-BACKPORT.md) until the complete supported runtime graph moves to a patched generation. Removing `proc-macro-error` alone does not justify removing the GLib correction.

## Reproduce and revisit

From the repository root with `cargo-audit 0.22.2` available:

```sh
cargo audit --version
cargo audit --file src-tauri/Cargo.lock --json > rust-audit.json
cargo audit --file scripts/fixtures/keyring-compat/Cargo.lock --json > keyring-reference-audit.json
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i proc-macro-error@1.0.4
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i ttf-parser@0.25.1
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zbus_names@4.3.5
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zvariant@5.15.1
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zvariant_derive@5.15.1
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target x86_64-unknown-linux-gnu -i dbus-secret-service@4.1.0
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target x86_64-unknown-linux-gnu -i zbus@5.19.0
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target x86_64-unknown-linux-gnu -i zbus@6.0.0
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i pipewire@0.10.1
```

Keep generated reports outside commits. Record the database revision, audit tool version, package/version counts and selected feature paths when updating this document. A later dependency release or advisory change invalidates the dated conclusions above.

Recheck this matrix with each Rust dependency update and before a release, especially when pdf-extract publishes the lopdf migration, opener publishes zbus 6 support, or Tauri publishes a supported GTK4 runtime. For each migration:

1. Update through its direct consumer; inspect manifest constraints and the resulting full locked graph.
2. Re-run the full audit without ignores. Verify both old and new versions rather than declaring the family gone after removing one path.
3. Run the normal frontend and Windows/macOS/Linux native CI, including the GLib source check and optimized Linux regression while the patch remains. A PDF change also needs a local PDF extraction fixture; credential-store changes need synthetic-entry migration fixtures.
4. Update this matrix and issue #19 with the exact final-head CI and remaining warnings. Close the issue only after all five families are removed through supported paths, or a maintainer explicitly accepts a narrower tracking outcome.

The 2026-10-20 source review ran metadata resolution and full production/reference audits. Native compatibility CI for this migration must pass at the final PR head before merge; authoring the fixture is not evidence that it passed. The unchanged GTK/PDF/opener migration candidates were not compiled, and no live provider validation is claimed.

## Issue #19 scope and remaining work

The maintainer's issue follow-ups keep #19 open until the five remaining families are removed through supported migrations. The closure policy above remains unchanged. The old production zbus 4 removal is prior progress; this audit and source refresh does not complete or close the issue, and none of the five remaining families is reported as removed or fixed.

The concrete remaining work is a published compatible GTK/GLib consumer graph, a published opener migration to zbus 6, and a published pdf-extract migration away from lopdf 0.42. Each still needs the full audits, actual selected reverse graphs and final-head platform/fixture checks listed above. No current platform test result is claimed by this audit refresh. Recheck at those consumer releases and before each OrbitPal release.
