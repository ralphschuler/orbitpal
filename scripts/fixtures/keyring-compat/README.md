# Native credential compatibility fixture

This non-publishable reference program compares OrbitPal's former `keyring 3.6.3`
API with the actual new [`credentials.rs`](../../../src-tauri/src/credentials.rs)
module. It is not part of the app workspace or production lockfile, and is never
bundled in an installer. Both APIs execute inside the same binary, avoiding
different macOS executable ACL identities; app signing continuity is tested
separately.

The CI launcher runs 15 operations in fresh processes using the unchanged
`streamer-ai-cohost` service and one unpredictable UUID credential ID. It verifies
old-write/new-read, new-write/old-read, updates in both directions, Unicode
(including supplementary code points), persistence across process exits,
deletion by both APIs and missing-entry behavior. Linux retains the old
`target=default` attribute. This fixture covers OrbitPal's keyring 3 entries,
not a pre-OrbitPal keyring 1 migration.

## Isolation

Do not run native migration fixtures on a developer's desktop. The launcher and
program require explicit disposable GitHub-hosted CI settings; self-hosted
runners and ordinary local invocations are rejected before native access.
No production secrets or signing inputs are needed or accepted.

- macOS uses a new temporary file-based keychain as the User default. The prior
  default and complete search list are recorded before mutation and restored in
  `finally`; the workflow also runs an `always()` recovery step. No login
  keychain items are read or enumerated.
- Linux uses a new DBus session, private HOME/XDG directories and a private GNOME
  Keyring unlocked with a public synthetic password. It cannot attach to the
  desktop's existing Secret Service.
- Windows reads, writes and deletes only the exact UUID target in the disposable
  runner account. It never enumerates the Credential Manager. Cleanup deletes
  the synthetic entry even if an assertion fails.

The public fixture markers are synthetic, and failures never render returned
credential values or backend error payloads. Normal app unit tests use an
in-memory mock store and do not invoke this launcher.

## Validation and advisories

The repository's three-OS CI runs:

```sh
node scripts/test-keyring-compat-ci.mjs
```

The macOS job additionally runs `node scripts/test-keyring-compat-ci.mjs --cleanup`
with `always()`. A fixture's source being present is not proof that a native
compatibility run passed; inspect the exact PR-head Windows, macOS and Linux
checks before merging.

The separately pinned legacy reference intentionally retains the old optional
async dependency graph. On 2026-10-20, cargo-audit 0.22.2 with RustSec revision
`999ec89d5d3f10f20be14750ec8c2d8b44e0ac9b` scanned 167 locked entries and reported
zero vulnerabilities plus three maintenance warnings: `zbus_names 3.0.0`
(`RUSTSEC-2026-0371`), `zvariant 4.2.0` (`RUSTSEC-2026-0370`) and
`zvariant_derive 4.2.0` (`RUSTSEC-2026-0372`). These optional legacy versions are
absent from the migrated production lockfile. The advisory workflow audits both
locks explicitly, without ignores:

```sh
cargo audit --file src-tauri/Cargo.lock
cargo audit --file scripts/fixtures/keyring-compat/Cargo.lock
```
