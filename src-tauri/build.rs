fn main() {
    let msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if msvc {
        // Tauri's normal resource embedding is scoped to application binaries.
        // Our lib unit-test executable also reaches Common Controls imports.
        // Match Tauri's own test harness: one manifest, linked into all targets.
        // https://github.com/tauri-apps/tauri/blob/dev/examples/api/src-tauri/build.rs
        // rustc-link-arg-tests alone does not cover lib unit-test executables.
        // https://github.com/rust-lang/cargo/issues/10937
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("OrbitPal Windows resources could not be built");
        let manifest = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory missing"),
        )
        .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    } else {
        tauri_build::build();
    }
}
