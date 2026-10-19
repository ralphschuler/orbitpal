# GLib 0.18.5 security backport

This is the complete published `glib` **0.18.5** crate, with its original
LICENSE, COPYRIGHT, source and tests. Its version is deliberately unchanged.
There are no dependency upgrades, generated build caches or copied credentials.

## Provenance

- Original archive: https://crates.io/api/v1/crates/glib/0.18.5/download
- Original archive SHA-256:
  `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`
- Original archive size: 267,679 bytes; 121 regular files.
- Advisory: https://rustsec.org/advisories/RUSTSEC-2024-0429.html
  (`GHSA-wrw7-89jp-8q8g`).
- Original upstream fix:
  https://github.com/gtk-rs/gtk-rs-core/pull/1343
- Upstream patch commit:
  `b5a4071e439bef2b5eea76c3aa25e5ae84839e34`
- Original patch:
  https://github.com/gtk-rs/gtk-rs-core/pull/1343.patch

The archive checksum was verified before extraction. Only
`src/variant_iter.rs` differs from that archive, by these two lines. This
provenance file, `COHOST-SECURITY.patch`, `COHOST-SHA256SUMS.txt` and
`cohost-regression.rs` are explicit app-maintained additions. The checksum
manifest covers all 121 original files after the exact patch plus our regression.

```diff
-            let p: *mut libc::c_char = std::ptr::null_mut();
+            let mut p: *mut libc::c_char = std::ptr::null_mut();
@@
-                &p,
+                &mut p,
```

`g_variant_get_child` writes through its variadic out-pointer. A mutable local
and mutable reference preserve that write under Rust optimization, preventing
the null-pointer dereference described by the advisory. Public APIs and GTK
0.18 compatibility are unchanged.

## Why this is carried locally

The locked Linux Tauri/GTK3/WebKitGTK graph requires `glib ^0.18`. The last
published 0.18 release is 0.18.5 and its official branch lacks this backport.
Forcing 0.20 would mix incompatible binding generations. A crates.io patch in
the app manifest selects this source instead, without falsifying its version.
Windows and macOS do not use this Linux GTK runtime dependency.

No consumer call to `Variant::array_iter_str` was found in the app or the
available dependency sources during the static review. That observation is
not a formal reachability proof and is not used as a substitute for this fix.
Version-based scanners can still report 0.18.5; this document records the
actual source remediation rather than claiming every scanner will be clear.

## Regression and maintenance

From the repository root, on Linux with the normal Tauri prerequisites:

```sh
(cd vendor/glib && sha256sum --check COHOST-SHA256SUMS.txt)
cargo test --locked --release --manifest-path src-tauri/Cargo.toml --test glib_variant_regression
```

This invokes the patched runtime API for `next`, `nth`, `last`, `next_back`
and `nth_back`, including UTF-8 and empty string elements. Release optimization
is required because the original defect is compiler-optimization sensitive.
The test does not start GTK, a webview or any network/model provider. It is a
separate app integration test to avoid resolving GLib's unrelated upstream
development/benchmark dependencies.

CI must retain this optimized Linux regression, the source-drift check and the
original crate/license provenance. Remove this patch only when the complete
Tauri GTK dependency graph supports an officially patched compatible release.
Do not edit files in the Cargo registry cache or increase the vendored version
merely to change vulnerability-scanner output.

The separate `proc-macro-error` maintenance warning still enters this GTK
generation through `glib-macros` and `gtk3-macros`. Its presence does not undo
this source correction, and its removal alone would not make the correction
obsolete. Track the complete supported runtime migration in the
[dependency maintenance matrix](../../docs/DEPENDENCY-MAINTENANCE.md).
