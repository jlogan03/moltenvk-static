use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=scripts/moltenvk.py");
    for name in [
        "MOLTENVK_STATIC_DIR",
        "MOLTENVK_ARTIFACT_URL",
        "MOLTENVK_ARTIFACT_SHA256",
        "MACOSX_DEPLOYMENT_TARGET",
        "DEVELOPER_DIR",
        "PYTHON",
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
    let version = env::var("MACOSX_DEPLOYMENT_TARGET")
        .expect("set MACOSX_DEPLOYMENT_TARGET=14.0 (or later) for the final application too");
    assert!(
        version
            .split('.')
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .is_some_and(|v| v >= 14),
        "moltenvk-static requires macOS 14 or later"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut command = Command::new(env::var_os("PYTHON").unwrap_or_else(|| "python3".into()));
    command.args(["scripts/moltenvk.py", "prepare"]).arg(&out);
    if env::var_os("CARGO_FEATURE_BUILD_MOLTENVK").is_some() {
        command.arg("--source");
    }
    assert!(
        command.status().expect("could not run Python 3").success(),
        "MoltenVK preparation failed"
    );
    let lib = out.join("artifact");
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
