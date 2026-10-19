# Rust dependency maintenance

This is the reproducible follow-up for [issue #19](https://github.com/ralphschuler/orbitpal/issues/19), reviewed on **2026-10-19** against the combined 0.9.0 source lockfile, including the Wayland portal dependencies. These entries are maintenance advisories. They are not evidence that an exploit was demonstrated in OrbitPal, and keeping this record does not fix the affected dependencies.

## Audit snapshot

`cargo-audit 0.22.2` fetched the RustSec database online before scanning. The database's remote HEAD and the report's database revision both resolved to `999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b` (2026-10-17). The scan contained 1,338 advisories and 637 locked package entries, with **0 vulnerability findings and 8 unmaintained package/version warnings across 5 advisory IDs**. No advisory IDs, architectures or operating systems were excluded. The lockfile SHA-256 was `185362a59a011f72f34818406b4ac45100385e726c379557ace9dd2514958918`.

This is a dated result, not a permanent clean bill of health. `cargo audit` scans the lockfile, which includes optional package dependencies. `cargo tree` with OrbitPal's selected features is a separate check of the active build graph; it must not be used to hide warnings from the full audit.

| Advisory | Exact locked versions | Selected build path | Supported migration and upstream status at review |
| --- | --- | --- | --- |
| [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html), `proc-macro-error` | `1.0.4` | Linux GTK: `glib-macros 0.18.5 → glib 0.18.5`; also `gtk3-macros 0.18.2 → gtk 0.18.2`. Both feed Tauri 2's GTK/WebKitGTK stack. | New GLib generations removed this macro dependency ([gtk-rs-core #1288](https://github.com/gtk-rs/gtk-rs-core/pull/1288)), but Tauri 2.12.4 and WebKitGTK still require the 0.18 generation. Track [Tauri GTK4 #7335](https://github.com/tauri-apps/tauri/issues/7335) and [migration PR #14684](https://github.com/tauri-apps/tauri/pull/14684), both open. |
| [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html), `ttf-parser` | `0.25.1` | All desktop platforms: `OrbitPal → pdf-extract 0.12.1 → lopdf 0.42.0 → ttf-parser`. | `lopdf 0.45` replaced the parser with optional `skrifa` ([lopdf #556](https://github.com/J-F-Liu/lopdf/pull/556), merged). The released `pdf-extract 0.12.1` still requires `lopdf ^0.42`; [pdf-extract #156](https://github.com/jrmuizel/pdf-extract/issues/156) and [PR #163](https://github.com/jrmuizel/pdf-extract/pull/163) remain open. PR #163 includes an API adaptation and a proposed `pdf-extract 0.13.0` release. |
| [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html), `zbus_names` | `3.0.0`, `4.3.5` | `4.3.5`: Linux `OrbitPal → tauri-plugin-opener 2.7.2 → zbus 5.19.0 / zbus_macros 5.19.0`. `3.0.0` is in the optional keyring graph below. | Names moved to `zbus::names` in zbus 6; see [zbus #2043](https://github.com/z-galaxy/zbus/issues/2043) (closed, migration released). Opener still requires `zbus ^5.9`; [plugins-workspace #3716](https://github.com/tauri-apps/plugins-workspace/issues/3716) tracks the open downstream migration. |
| [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html), `zvariant` | `4.2.0`, `5.15.1` | `5.15.1`: the same selected opener/zbus 5 graph, including `zbus_names 4.3.5`. `4.2.0` is in the optional keyring graph below. | D-Bus types moved to `zbus::wire` in zbus 6; GVariant is a separate project. The same [opener migration #3716](https://github.com/tauri-apps/plugins-workspace/issues/3716) is required. An upgrade within zbus 5 retains this warning. |
| [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html), `zvariant_derive` | `4.2.0`, `5.15.1` | Each version is pulled by the corresponding `zvariant` version above. | Derives moved into zbus 6. The same downstream migration is required; do not substitute zbus 6 underneath APIs compiled against zbus 4/5. |

The [ttf-parser repository](https://github.com/harfbuzz/ttf-parser) now describes bug-fix-only maintenance, while the current RustSec entry still marks the published crate unmaintained. That changed upstream wording is not a reason to suppress the advisory or claim a new fixed release.

### Optional keyring graph

The lockfile also records `keyring 3.6.3 → secret-service 4.0.0 → zbus 4.4.0 → zbus_names 3.0.0 / zvariant 4.2.0 / zvariant_derive 4.2.0`. It is important to distinguish that resolved edge from an enabled backend:

- OrbitPal selects `sync-secret-service` and `crypto-rust` on Linux. [Keyring 3.6.3's feature manifest](https://github.com/open-source-cooperative/keyring-rs/blob/v3.6.3/Cargo.toml) selects `dbus-secret-service 4.1.0` for that backend, without zbus.
- `secret-service` and zbus 4 belong to keyring's unselected `async-secret-service` feature. Reverse trees with `--target all` and specifically `x86_64-unknown-linux-gnu` have no active path to those old versions. The full lockfile audit still reports them.
- [Keyring 4.2.0](https://github.com/open-source-cooperative/keyring-rs/releases/tag/v4.2.0) is a published alternative with split backend crates, not a compatible update to keyring 3's feature configuration. Evaluate it as a separate credential-store migration, including an isolated synthetic-entry compatibility check on all three operating systems. Preserve the existing service namespace and secret IDs; never inspect real credentials for a migration test.

### Separate Wayland portal graph

The integrated Wayland implementation adds a direct `OrbitPal → zbus 6.0.0` path and `pipewire 0.10.1`. Its 24 additional locked packages increase the previous snapshot from 613 to 637 entries; they do not migrate opener or keyring. Linux reverse trees confirm that opener still selects zbus 5.19.0 alongside the new direct zbus 6 path. The optional keyring/zbus 4 family remains in the full lockfile. Having zbus 6 installed therefore does not resolve the older families' maintenance warnings.

## Upgrade decisions

Fresh crates.io metadata at review confirmed `tauri 2.12.4`, `tauri-plugin-opener 2.7.2` and `pdf-extract 0.12.1` as the latest published stable versions, and `keyring 3.6.3` as the latest published 3.x version. The published consumer constraints still require `lopdf ^0.42` and opener's `zbus ^5.9`. No compatible released upgrade removed these five advisory families. This audit refresh changes no dependency versions or features.

Do not force GTK/GLib binding generations, patch transitive crates to incompatible majors, point production dependencies at unreviewed upstream branches, replace the persistent Linux credential backend with a volatile one, or delete optional lockfile entries by hand. These changes would either break supported behavior or make the audit record misleading.

The existing GLib source correction addresses the separate `RUSTSEC-2024-0429` defect. Keep the [backport provenance, exact source hashes and optimized iterator regression](../vendor/glib/COHOST-BACKPORT.md) until the complete supported runtime graph moves to a patched generation. Removing `proc-macro-error` alone does not justify removing the GLib correction.

## Reproduce and revisit

From the repository root with `cargo-audit 0.22.2` available:

```sh
cargo audit --version
cargo audit --file src-tauri/Cargo.lock --json > rust-audit.json
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i proc-macro-error@1.0.4
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i ttf-parser@0.25.1
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zbus_names@3.0.0
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zbus_names@4.3.5
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zvariant@4.2.0
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zvariant@5.15.1
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zvariant_derive@4.2.0
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target all -i zvariant_derive@5.15.1
cargo tree --manifest-path src-tauri/Cargo.toml --locked --target x86_64-unknown-linux-gnu -i zbus@4.4.0
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

This review ran the full current audit and reverse dependency trees; it did not compile an alternative GTK, keyring or PDF stack, exercise an OS credential store, or claim live provider validation. No runtime code or dependency versions changed.
