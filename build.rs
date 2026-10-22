use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for name in [
        "MOLTENVK_STATIC_DIR",
        "MOLTENVK_ARTIFACT_URL",
        "MOLTENVK_ARTIFACT_SHA256",
        "DEVELOPER_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let target = env::var("TARGET").unwrap();
    if target != "aarch64-apple-darwin" {
        // Permit workspace checks on other platforms without downloading native code.
        println!(
            "cargo:warning=moltenvk-static has no API on {target}; only aarch64-apple-darwin is supported"
        );
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let lib = moltenvk_build::prepare(&out, env::var_os("CARGO_FEATURE_BUILD_MOLTENVK").is_some())
        .unwrap_or_else(|error| panic!("MoltenVK preparation failed: {error:#}"));
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=static=MoltenVK");
    println!("cargo:rustc-link-lib=dylib=c++");
    for framework in [
        "Metal",
        "Foundation",
        "QuartzCore",
        "CoreGraphics",
        "IOSurface",
        "IOKit",
        "AppKit",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    if let Some(local) = env::var_os("MOLTENVK_STATIC_DIR") {
        println!("cargo:rerun-if-changed={}", PathBuf::from(local).display());
    }
}
