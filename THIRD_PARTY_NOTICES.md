# Third-party notices

The root MIT license covers original application code, not a relicensing of third-party material.

- **3D assets:** source authors, license texts, hashes and required Fox CC BY 4.0 attribution are in [public/characters/ATTRIBUTION.md](public/characters/ATTRIBUTION.md) and [public/characters/osa/ATTRIBUTION.md](public/characters/osa/ATTRIBUTION.md). These files are bundled with the app. Retain notices when redistributing.
- **Fonts:** Manrope and Space Grotesk are supplied by their Fontsource packages under the SIL Open Font License. Their notices are bundled under `public/licenses/`.
- **JavaScript and Rust dependencies:** exact versions are recorded in `pnpm-lock.yaml` and `src-tauri/Cargo.lock`; each dependency retains its published license. [DEPENDENCIES.txt](public/licenses/DEPENDENCIES.txt) reproduces installed LICENSE, NOTICE, COPYING, COPYRIGHT and AUTHORS files, published author metadata and source copyright headers, including development and platform-specific packages. It is bundled with the app.

Run `node scripts/third-party-notices.mjs --strict` after installing JavaScript dependencies and running `cargo fetch --locked`, and before building. The generator uses offline Cargo metadata, writes no local manifest paths, and makes no provider calls. CI and release builds regenerate this file.

Some upstream package archives omit a standalone license file. For those packages the collection identifies the declared license choice and supplies its complete standard terms from the pinned official SPDX License List v3.27.0, alongside available original copyright headers, authors and the corresponding versioned package-source link. SPDX copyright placeholders remain explicitly marked as template fields; author names or years are never invented. An unknown or unsupported missing license text causes strict generation to fail. Original asset and font licenses remain separate.

Imported user models, voices, documents and extensions are not distributed by this repository. Their users must ensure they have the required rights.
