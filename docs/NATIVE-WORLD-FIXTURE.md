# Native World window fixture

This nonshipping macOS example runs the production Wry frontend, handlers,
window constructors and availability monitor with a fresh synthetic Store.
It cannot use installed OrbitPal data or OS credentials. Provider, CLI, memory,
MCP and platform calls fail before outbound I/O. Ordinary builds never enable
this feature, and the ordinary app entry refuses to run when it is enabled.

Build the frontend, then explicitly select an existing absolute, canonical,
owned evidence directory with no group/other write access:

```sh
pnpm build
python3 scripts/native-world-fixture/build.py \
  --evidence-dir '/absolute/evidence/directory'
```

The helper passes the isolated fixture JSON through `TAURI_CONFIG` to the same
Cargo invocation, so Tauri's build metadata and `generate_context!` use the
same overlay. The macro's filename argument selects a configuration directory;
it does not by itself load this alternate filename. Explicit JSON nulls remove
the shipping development URL, build hooks and plugin configuration during
JSON Merge Patch. A plain Cargo command without the helper is insufficient.
The helper uses one Cargo job and inherits the caller's Cargo/Rust toolchains
and target directory. It does not change HOME/XDG settings or launch the app.
Existing runtime isolation checks remain unchanged and refuse an unsafe
compiled configuration before constructing a WebView.

The distinct app identifier and nonpersistent WK data stores isolate both Main
and the actual overlay constructors. The example creates a new private run
folder and seeds two local Nova profiles. It exposes the normal Studio UI;
choose both participants, place them in the visual editor and save/start Life.
Profile saving accepts only the unchanged sealed synthetic profiles.

The read-only recorder observes real preparation, accepted frames and exact
physical cleanup. Each actor needs a continuous accepted-frame span of at least
35 seconds, at least 10 seconds with Main actually hidden, and a final actual
Main Stop followed by physical restoration/retirement and window destruction.
Owner recreation during hide/show keeps separate epoch measurements. Quit
through the normal application controls after cleanup. The fixture expires
after 180 seconds and creates only a bounded numeric/enum result file.

```sh
python3 scripts/native-world-fixture/test-validator.py
python3 scripts/native-world-fixture/validate.py /exact/run/folder/result.json
```

Exit 0 from the validator qualifies only `actual_wry_native_render_lifecycle`.
Missing observations remain incomplete. This fixture does not qualify native
movement/slot arrivals, speech handoff, OBS/CEF, mixed DPI, lock/suspend or GPU
pixel presentation. Source tests and a normal process exit do not prove any of
those behaviors. No successful native run is claimed by these instructions.
