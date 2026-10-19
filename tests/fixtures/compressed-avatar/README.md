# Original offline compression fixtures

These tiny red triangles and textures are original OrbitPal test data under the project MIT license, generated without downloaded avatars, remote resources or user files. They contain no accounts, recordings or personal data.

Regenerate with `python3 scripts/generate-compressed-avatar-fixtures.py --blender /path/to/blender --meshoptimizer /path/to/libmeshoptimizer` (Blender 5.2.1 was used for the checked-in Draco stream). The encoder tools are not runtime dependencies.

- `draco`: actual Blender Draco-compressed geometry.
- `meshopt-ext` / `meshopt-khr`: actual native Meshoptimizer encoder output, with required, optional and URI-less virtual fallback-buffer cases.
- `basisu`: original 4×4 solid-red UASTC LDR block in a KTX2 container, authored directly from the UASTC/KTX2 layout. `basisu-optional` includes an original PNG fallback. `basisu-external.gltf` has separate local BIN/KTX2 resources.
- `invalid-draco-count.glb`: the original three-vertex Draco stream with a deliberately false two-vertex declaration. The decoded-scene check must reject it; no large allocation is needed.
- JSON-glTF sources contain exactly those streams, so the native importer and render integration share actual data.

`cargo test` validates/imports these files. With `ORBITPAL_NATIVE_FIXTURE_DIR` set to a temporary output directory, the import fixture writes ten canonical GLB results there. `pnpm test:avatar-decoders` then renders those exact native outputs, the original GLB/glTF sources, GPU-format and forced-RGBA texture targets, the Studio/transparent stage paths and valid optional fallbacks without decoders. Each drawing must contain the expected red pixels. Both actual CSP strings are applied by the local fixture server; network requests are observed and external requests/CSP violations fail the run. CI runs this on all three OS jobs with their installed Chrome. This does not emulate OBS's own CEF or the platform WebView/GPU driver.

The checked-in texture fixture covers UASTC LDR. ETC1S/BasisLZ is accepted by the same reviewed upstream transcoder and bounded container validation, but is not a separately authored encoder fixture. HDR KTX2, 3D/array/cubemap textures and arbitrary native GPU KTX formats are outside `KHR_texture_basisu` support here.

Primary format references: [KHR_draco_mesh_compression](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_draco_mesh_compression), [KHR_meshopt_compression](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_meshopt_compression), [KHR_texture_basisu](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_texture_basisu), [UASTC layout](https://github.com/BinomialLLC/basis_universal/wiki/UASTC-Texture-Specification).
