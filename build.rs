use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // These directives tell Cargo to rerun this build script whenever the specified files change.
    // This ensures that if the runtime crate's Cargo.toml or main library file is modified,
    // this build script will be executed again to rebuild the runtime dependency.
    println!("cargo:rerun-if-changed=targets/x86_64_linux/runtime/Cargo.toml");
    println!("cargo:rerun-if-changed=targets/x86_64_linux/runtime/src/lib.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let profile = env::var("PROFILE").expect("PROFILE not set");

    let mut rustflags = env::var("RUSTFLAGS").unwrap_or_default();
    if !rustflags.contains("panic=abort") {
        if !rustflags.trim().is_empty() {
            rustflags.push(' ');
        }
        rustflags.push_str("-C panic=abort");
    }

    let mut command = Command::new(&cargo);
    let runtime_target_dir = manifest_dir.join("target").join("runtime-build");
    let telemetry_enabled = env::var("CARGO_FEATURE_ALLOCATOR_TELEMETRY").is_ok();
    let debug_enabled = env::var("CARGO_FEATURE_DEBUG").is_ok();

    command
        .current_dir(&manifest_dir)
        .env("RUSTFLAGS", rustflags)
        .env("CARGO_TARGET_DIR", &runtime_target_dir)
        .env_remove("CARGO_MAKEFLAGS")
        .env_remove("MAKEFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args(["build", "-p", "slisp-runtime", "--no-default-features"]);

    let mut features = Vec::new();
    if telemetry_enabled {
        features.push("telemetry");
    }
    if debug_enabled {
        features.push("debug");
    }

    if !features.is_empty() {
        command.args(["--features", &features.join(",")]);
    }

    match profile.as_str() {
        "release" => {
            command.arg("--release");
        }
        "debug" => { /* dev profile, no extra flag needed */ }
        other => {
            command.args(["--profile", other]);
        }
    }

    let status = command.status().expect("Failed to invoke cargo build for slisp-runtime");

    if !status.success() {
        panic!("Building slisp-runtime failed with status {}", status);
    }

    let profile_dir = profile.as_str();

    let lib_path = runtime_target_dir.join(profile_dir).join("libslisp_runtime.a");

    if !lib_path.exists() {
        panic!("Expected runtime static library at {}, but it was not found", lib_path.display());
    }

    let lib_path = lib_path.canonicalize().unwrap_or_else(|_| lib_path.clone());

    println!("cargo:rustc-env=SLISP_RUNTIME_LIB={}", lib_path.display());
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");
}
