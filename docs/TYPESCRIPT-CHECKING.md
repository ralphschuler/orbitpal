# TypeScript checking

OrbitPal uses the [supported Svelte TypeScript 7 setup](https://github.com/sveltejs/language-tools/blob/master/packages/svelte-check/README.md#typescript-7-supports): two compiler dependencies with different jobs, resolved by the same frozen pnpm lockfile.

| Dependency | Purpose |
| --- | --- |
| `@typescript/native`, an npm alias for `typescript@^7.0.2` | Native TypeScript 7 compiler used by `svelte-check --tsgo`. |
| `typescript@^6.0.3` | JavaScript compiler APIs used by Svelte tooling, editors and the compatibility check. TypeScript 7 does not replace these APIs. |

`pnpm check` runs Svelte diagnostics and native TypeScript 7 checking against `tsconfig.json`. `pnpm check:legacy` runs the same project through the JavaScript TypeScript 6 checker. CI runs both on Windows, macOS and Linux, followed by the existing Vitest, frontend build and native checks. Keep both checks green; do not hide a disagreement by excluding source files or changing strictness for one compiler.

Editors that request a workspace TypeScript SDK should use `node_modules/typescript/lib`, the TypeScript 6 JavaScript API. Do not point them at the native compiler alias. The Svelte editor extension continues to provide Svelte language support. This configuration does not change the Vite bundler, Vitest runtime or Rust compiler.

The native checker creates a temporary `.svelte-check/` workspace. It is ignored by Git, is not an application asset, and can be removed before rerunning the check. A normal frozen install selects the native compiler binary for the developer's operating system; retain the optional platform packages in `pnpm-lock.yaml` for CI and other contributors.

## Verification

```sh
pnpm install --frozen-lockfile
pnpm check
pnpm check:legacy
pnpm test:run
pnpm build
```

The existing strict dependency-notice generation also scans installed development tooling before release packaging. Its platform-dependent output is regenerated in CI; a checker-only change should not remove another platform's checked-in notices merely because those packages are absent from a fresh local installation.
