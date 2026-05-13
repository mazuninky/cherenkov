//! Build script: generates Rust types from `proto/v1.proto` via `prost-build`.
//!
//! `protoc` is provided by `protoc-bin-vendored` so the build is hermetic and
//! does not require the user to install Protobuf compiler binaries.

fn main() -> std::io::Result<()> {
    let protoc = protoc_bin_vendored::protoc_bin_path()
        .expect("protoc-bin-vendored ships a protoc binary for this platform");
    // SAFETY: build scripts run before any threads spawn, so racing with
    // a concurrent getenv is not possible. Required by edition 2024.
    unsafe { std::env::set_var("PROTOC", protoc) };

    println!("cargo:rerun-if-changed=proto/v1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    prost_build::compile_protos(&["proto/v1.proto"], &["proto"])?;
    Ok(())
}
