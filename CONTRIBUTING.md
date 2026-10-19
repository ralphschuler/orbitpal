# Contributing

People and AI-assisted contributions follow the same review and quality requirements. Please open an issue before a large architectural change; focused fixes can go straight to a pull request. English or German is welcome.

## Set up

Use Node.js 22.12 or newer, Corepack/pnpm pinned by `package.json`, stable Rust, and the native dependencies in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Linux needs WebKitGTK 4.1, GTK, AppIndicator and DBus development packages. Windows needs Microsoft C++ Build Tools and WebView2; macOS needs Xcode Command Line Tools.

```sh
git clone https://github.com/ralphschuler/orbitpal.git
cd orbitpal
corepack enable
pnpm install --frozen-lockfile
pnpm tauri dev
```

## Before submitting

```sh
pnpm check
pnpm test:run
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Add focused regression tests for behavior changes, using local fixtures or mock servers. Tests must not call paid providers, connect real stream accounts, record microphones or read real credentials. Update documentation and both lockfiles when dependencies change. Run native builds for platform-specific changes when possible, and report platforms you could not check.

Keep changes focused. Explain the user-visible problem, the resulting behavior, validation and limitations in the PR. Do not include app data, exported profiles, chat transcripts, provider recordings, secrets or compiled installers. New assets need a source, redistribution license and attribution. Do not claim measured performance or live provider validation without evidence.

## Branches and review

Create a focused branch and open a pull request against `main`. Main requires an up-to-date branch and these GitHub checks:

- `Checks (ubuntu-24.04)`
- `Checks (windows-latest)`
- `Checks (macos-latest)`
- `Dependency advisories`

One approving review and resolution of review conversations are required. New commits dismiss stale approvals. Use squash merges to preserve linear history; force pushes and deletion of `main` are blocked. Review the final PR head and combined dependency changes before merging. A previously green run on a different head is insufficient.

Administrators retain bypass for recovery and explicitly authorized maintenance. Ordinary contributions must use the protected PR workflow. AI agents must not use administrator bypass, approve their own work, merge or publish without explicit maintainer authorization; the existence of credentials or a GitHub permission is not authorization.

CI also tests the release packaging scripts, generates strict third-party notices and runs the Linux GLib source-hash/optimized regression checks. The advisory workflow runs JavaScript and Rust dependency checks. Fix failing checks; do not disable them or add broad advisory exclusions. Track upstream maintenance limitations explicitly, as in the [2026-10-19 audit and follow-up issues](docs/AUDIT-2026-10-19.md).

## AI-assisted work

Read [AGENTS.md](AGENTS.md) before editing. The contributor remains responsible for the full diff, licensing and validation. Disclose substantial AI assistance and identify the checks actually performed. Never fabricate test results, provider responses or benchmark data. Repository instructions govern source contributions; per-character instruction files govern runtime bots and must not override repository or host security policies. Humans review and merge contributions; agents must not self-approve, publish releases or use maintainers' accounts without explicit authorization.

By contributing, you agree that your original contribution is available under the project's MIT license. Third-party assets retain their own documented licenses. No contributor license agreement is required.
