# Offline avatar decoders

All renderer paths use `src/lib/avatar-loader.ts` and decoder assets from **Three 0.186.1**, exactly matching `pnpm-lock.yaml`. There are no CDN URLs or runtime decoder downloads outside this application's origin. Draco's glTF build and Basis WASM/JS are derived from the installed npm package with the reviewed resource/CSP changes below; Meshopt is its bundled ES module with embedded WASM. One worker per codec/stage bounds concurrency. KTX2 hardware capabilities are detected on the actual renderer, including RGBA fallback.

`manifest.json` records original and distributed SHA-256 hashes, source paths and licenses. `node scripts/verify-avatar-decoders.mjs` runs before production builds. An update to Three or a decoder requires explicit source/hash/license review; changing the package version alone fails the build.

## Basis CSP modification

Three's supplied Basis wrapper uses Emscripten's `Function(...)` to generate its fixed Embind and Emval call adapters. This does not work under a CSP that blocks JavaScript evaluation. OrbitPal does **not** grant `unsafe-eval` to the app or OBS.

`node scripts/prepare-avatar-decoders.mjs --write` executes the reviewed wrapper/WASM **offline at development time**, records its 19 fixed adapter signatures during registration and the original KTX2 fixture, and writes ordinary static JavaScript functions. A single guard in `newFunc` dispatches Function construction to this table. An unknown signature fails explicitly; it never evaluates code. The remaining Basis wrapper code is unchanged. The separate WASM memory cap below changes the memory declaration and adds a nonexecuting modification notice; executable WASM code/data remain unchanged. The modification is labeled in the JS file and manifest. Without `--write`, the script checks byte-for-byte reproducibility.

Both CSPs permit only `wasm-unsafe-eval` for WASM compilation and `worker-src blob:` for Three's locally constructed decoder workers. General `eval`, inline scripts and remote code remain blocked. Workers inherit the page CSP. Do not broaden these directives to fix a future decoder incompatibility.

## Memory and worker boundaries

`node scripts/prepare-avatar-wasm.mjs --write` changes the single declared wasm32 memory maximum of both Draco/Basis binaries from 2 GiB to **256 MiB** and adds an `orbitpal.notice` custom section. Initial memory, imports, executable code and data are unchanged. Original and patched hashes are separate. A check without `--write` verifies reproduction. `avatar-wasm-memory-tests.mjs` instantiates each real patched module and rejects growth over the maximum without a boundary-sized allocation.

`node scripts/prepare-avatar-draco.mjs --write` adds a wrapper hook before Draco's `onModuleLoaded` callback. It checks the real aggregate aligned attribute and Uint32-index output before JavaScript allocation/copy, capped at 256 MiB across all primitives of a model. Trusted per-load task tokens bind input ArrayBuffers to a shared model budget even when tasks queue before WASM initialization. The budget is released only when that GLTF load settles; worker retirement releases it on faults. Reservation happens after bounded `wasm_malloc` and before JS output copies. Repeated attribute aliases cannot multiply output past the bound; metadata counts are not trusted as the decoded size. The original wrapper's remaining code is unchanged.

The loader rejects tasks before retiring workers on faults, a 60-second deadline or disposal. It blocks late worker creation through the pinned Three task boundaries and disposes late scenes/Blob URLs. A retry uses fresh codecs. These bounds do not represent an overall process/GPU memory quota.

## Licenses and provenance

- Three: MIT (`LICENSE-THREE-MIT.txt`). Source: https://www.npmjs.com/package/three/v/0.186.1
- Draco: Apache-2.0; glTF decoder with the WASM/output-budget modifications above; upstream full license/third-party notices and README. https://github.com/google/draco
- Basis Universal: Apache-2.0; the wrapper modification described above, full upstream license and README. https://github.com/BinomialLLC/basis_universal
- Emscripten glue: MIT / University of Illinois/NCSA terms, preserved in `basis/LICENSE-EMSCRIPTEN.txt`. https://github.com/emscripten-core/emscripten
- Zstandard decoder included in Basis: BSD-3-Clause terms preserved in `basis/LICENSE-ZSTD.txt`. https://github.com/facebook/zstd
- Meshoptimizer 1.1 (as stated in the supplied Three module): MIT, copyright 2016–2026 Arseny Kapoulkine; its source header and complete MIT license are retained in generated dependency notices.

Insignificant trailing whitespace in the Emscripten license was normalized; original/distributed hashes are recorded and its complete terms are unchanged.

The upstream Draco/Basis versions are identified by the **exact shipped file hashes**, rather than an inferred version from current upstream branches. Original development fixtures and their generator are in `tests/fixtures/compressed-avatar` and carry this project's MIT license; they do not contain downloaded character assets. Generated dependency notices also include these full license texts and hashes.
