# Rust dependency maintenance

This is the reproducible follow-up for [issue #19](https://github.com/ralphschuler/orbitpal/issues/19), reviewed on **2026-10-20** against the 0.10.0 source with the explicit native credential-store migration and Wayland portal dependencies. These entries are maintenance advisories. They are not evidence that an exploit was demonstrated in OrbitPal, and keeping this record does not fix the affected dependencies.

## Audit snapshot

`cargo-audit 0.22.2` fetched the RustSec database online before scanning. The database's remote HEAD and the report's database revision both resolved to `999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b` (2026-10-17). The scan contained 1,338 advisories and 625 locked package entries, with **0 vulnerability findings and 5 unmaintained package/version warnings across 5 advisory IDs**. No advisory IDs, architectures or operating systems were excluded. The lockfile SHA-256 was `a685b2cecd4d769b8c1523c9b08efb6901549ccbeab3368c6cdbba431bd03d01`.

This is a dated result, not a permanent clean bill of health. `cargo audit` scans the lockfile, which includes optional package dependencies. `cargo tree` with OrbitPal's selected features is a separate check of the active build graph; it must not be used to hide warnings from the full audit.

| Advisory | Exact locked versions | Selected build path | Supported migration and upstream status at review |
| --- | --- | --- | --- |
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html), `proc-macro-error` | `1.0.4` | Linux GTK: `glib-macros 0.18.5 → glib 0.18.5`; also `gtk3-macros 0.18.2 → gtk 0.18.2`. Both feed Tauri 2's GTK/WebKitGTK stack. | New GLib generations removed this macro dependency ([gtk-rs-core #1288](https://github.com/gtk-rs/gtk-rs-core/pull/1288)), but Tauri 2.12.4 and WebKitGTK still require the 0.18 generation. Track [Tauri GTK4 #7335](https://github.com/tauri-apps/tauri/issues/7335) and [migration PR #14684](https://github.com/tauri-apps/tauri/pull/14684), both open. |
| [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html), `ttf-parser` | `0.25.1` | All desktop platforms: `OrbitPal → pdf-extract 0.12.1 → lopdf 0.42.0 → ttf-parser`. | `lopdf 0.45` replaced the parser with optional `skrifa` ([lopdf #556](https://github.com/J-F-Liu/lopdf/pull/556), merged). The released `pdf-extract 0.12.1` still requires `lopdf ^0.42`; [pdf-extract #156](https://github.com/jrmuizel/pdf-extract/issues/156) and [PR #163](https://github.com/jrmuizel/pdf-extract/pull/163) remain open. PR #163 includes an API adaptation and a proposed `pdf-extract 0.13.0` release. |
| [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html), `zbus_names` | `4.3.5` | `4.3.5`: Linux `OrbitPal → tauri-plugin-opener 2.7.2 → zbus 5.19.0 / zbus_macros 5.19.0`. The old optional keyring/zbus 4 family was removed by the migration below. | Names moved to `zbus::names` in zbus 6; see [zbus #2043](https://github.com/z-galaxy/zbus/issues/2043) (closed, migration released). Opener still requires `zbus ^5.9`; [plugins-workspace #3716](https://github.com/tauri-apps/plugins-workspace/issues/3716) tracks the open downstream migration. |
| [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html), `zvariant` | `5.15.1` | `5.15.1`: the same selected opener/zbus 5 graph, including `zbus_names 4.3.5`. The old optional keyring/zbus 4 family was removed by the migration below. | D-Bus types moved to `zbus::wire` in zbus 6; GVariant is a separate project. The same [opener migration #3716](https://github.com/tauri-apps/plugins-workspace/issues/3716) is required. An upgrade within zbus 5 retains this warning. |
| [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html), `zvariant_derive` | `5.15.1` | Each version is pulled by the corresponding `zvariant` version above. | Derives moved into zbus 6. The same downstream migration is required; do not substitute zbus 6 underneath APIs compiled against zbus 4/5. |

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

Fresh crates.io metadata at review confirmed `tauri 2.12.4`, `tauri-plugin-opener 2.7.2` and `pdf-extract 0.12.1` as the latest published stable versions, and `keyring 3.6.3` as the latest published 3.x version. The published consumer constraints still require `lopdf ^0.42` and opener's `zbus ^5.9`. The credential-store migration above uses released, explicit providers and removes three warned package versions. The five remaining advisory families still require the GTK, opener or PDF consumer migrations in the table; no compatible published update currently removes them.

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
