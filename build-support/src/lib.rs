//! Acquire, build, and package the pinned MoltenVK archive.

use anyhow::{Context, Result, ensure};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use walkdir::WalkDir;

const VERSION: &str = "1.4.2";
const REVISION: &str = "db66022459ffb663aa2b50f6b018bc2e124f5edf";
const ARTIFACT: &str = "moltenvk-1.4.2-aarch64-apple-darwin-macos14.tar.gz";
const SOURCE_PATCH: &str = include_str!("../patches/swapchain-alias.patch");

fn baseline() -> Value {
    json!({
        "schema": 1,
        "revision": REVISION,
        "target": "aarch64-apple-darwin",
        "deployment_target": "14.0",
        "patches": { "swapchain-alias.patch": format!("{:x}", Sha256::digest(SOURCE_PATCH)) },
    })
}

fn validate(directory: &Path) -> Result<()> {
    let manifest: Value = serde_json::from_reader(File::open(directory.join("manifest.json"))?)?;
    for (key, value) in baseline().as_object().unwrap() {
        ensure!(
            manifest.get(key) == Some(value),
            "artifact {key} must be {value}"
        );
    }
    let mut magic = [0; 8];
    File::open(directory.join("libMoltenVK.a"))?.read_exact(&mut magic)?;
    ensure!(
        &magic == b"!<arch>\n",
        "libMoltenVK.a must be a thin static archive"
    );
    ensure!(
        directory.join("licenses").is_dir(),
        "artifact is missing licenses"
    );
    Ok(())
}

fn unpack(archive: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in tar::Archive::new(GzDecoder::new(File::open(archive)?)).entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        // Extract regular files only. Never follow links or accept parent paths.
        ensure!(
            path.components()
                .all(|part| matches!(part, Component::Normal(_) | Component::CurDir)),
            "invalid archive path: {}",
            path.display()
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "unsupported archive entry: {}",
            path.display()
        );
        ensure!(
            entry.unpack_in(destination)?,
            "invalid archive path: {}",
            path.display()
        );
    }
    validate(destination)
}

fn digest(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn download(url: &str, checksum: &str, out: &Path, destination: &Path) -> Result<()> {
    ensure!(
        checksum.len() == 64
            && checksum
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "MOLTENVK_ARTIFACT_SHA256 must be a lowercase SHA-256 digest"
    );
    ensure!(
        url.starts_with("https://"),
        "artifact downloads require HTTPS"
    );
    let cached = out.join(format!("{checksum}.tar.gz"));
    if !cached.exists() {
        let mut temporary = tempfile::NamedTempFile::new_in(out)?;
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(Duration::from_secs(120)))
            .build()
            .new_agent();
        let mut response = agent.get(url).call()?;
        io::copy(&mut response.body_mut().as_reader(), &mut temporary)?;
        temporary.persist(&cached)?;
    }
    if digest(&cached)? != checksum {
        fs::remove_file(cached)?;
        anyhow::bail!("artifact checksum mismatch");
    }
    unpack(&cached, destination)
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("could not run {command:?}"))?;
    ensure!(status.success(), "{command:?} failed: {status}");
    Ok(())
}

fn output(command: &mut Command) -> Result<String> {
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("could not run {command:?}"))?;
    ensure!(
        output.status.success(),
        "{command:?} failed: {}",
        output.status
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn build(work: &Path, destination: &Path) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "source builds require macOS with Xcode"
    );
    fs::create_dir_all(work)?;
    let source = work.join("MoltenVK");
    if !source.exists() {
        run(Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                &format!("v{VERSION}"),
                "https://github.com/KhronosGroup/MoltenVK.git",
            ])
            .arg(&source))?;
    }
    ensure!(
        output(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&source)
        )? == REVISION,
        "MoltenVK checkout does not match the pinned revision"
    );
    // Register the EXT name against its KHR implementation when exported aliases are hidden.
    let patch = work.join("swapchain-alias.patch");
    fs::write(&patch, SOURCE_PATCH)?;
    if !Command::new("git")
        .args(["apply", "--reverse", "--check"])
        .arg(&patch)
        .current_dir(&source)
        .output()?
        .status
        .success()
    {
        run(Command::new("git")
            .arg("apply")
            .arg(&patch)
            .current_dir(&source))?;
    }
    // Apply the same settings to upstream's nested dependency builds.
    let config = work.join("build.xcconfig");
    fs::write(
        &config,
        concat!(
            "ARCHS = arm64\nONLY_ACTIVE_ARCH = NO\n",
            "MACOSX_DEPLOYMENT_TARGET = 14.0\n",
            "CODE_SIGNING_ALLOWED = NO\nLLVM_LTO = NO\n",
            "GCC_SYMBOLS_PRIVATE_EXTERN = YES\n",
            "GCC_PREPROCESSOR_DEFINITIONS = $(inherited) MVK_HIDE_VULKAN_SYMBOLS=1\n",
        ),
    )?;
    run(Command::new("./fetchDependencies")
        .arg("--macos")
        .current_dir(&source)
        .env("XCODE_XCCONFIG_FILE", &config))?;
    run(Command::new("xcodebuild")
        .args([
            "build",
            "-project",
            "MoltenVKPackaging.xcodeproj",
            "-scheme",
            "MoltenVK Package (macOS only)",
            "-configuration",
            "Release",
            "-destination",
            "generic/platform=macOS",
        ])
        .current_dir(&source)
        .env("XCODE_XCCONFIG_FILE", &config))?;
    let mut libraries = Vec::new();
    for entry in fs::read_dir(source.join("Package/Release/MoltenVK/static/MoltenVK.xcframework"))?
    {
        let entry = entry?;
        let library = entry.path().join("libMoltenVK.a");
        if entry.file_name().to_string_lossy().starts_with("macos-") && library.is_file() {
            libraries.push(library);
        }
    }
    ensure!(
        libraries.len() == 1,
        "expected one macOS static library, found {libraries:?}"
    );
    let architectures = output(
        Command::new("xcrun")
            .args(["lipo", "-archs"])
            .arg(&libraries[0]),
    )?;
    ensure!(
        architectures == "arm64",
        "expected arm64 only, found {architectures}"
    );
    fs::copy(&libraries[0], destination.join("libMoltenVK.a"))?;
    let licenses = destination.join("licenses");
    fs::create_dir(&licenses)?;
    // Preserve notices from MoltenVK and its pinned source dependencies.
    let mut roots = vec![source.clone()];
    for entry in fs::read_dir(source.join("External"))? {
        roots.push(entry?.path());
    }
    for root in roots.into_iter().filter(|p| p.is_dir()) {
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry.path().is_file()
                && ["LICENSE", "COPYING", "NOTICE"]
                    .iter()
                    .any(|prefix| entry.file_name().to_string_lossy().starts_with(prefix))
            {
                let folder = licenses.join(root.file_name().unwrap());
                fs::create_dir_all(&folder)?;
                fs::copy(entry.path(), folder.join(entry.file_name()))?;
            }
        }
    }
    let mut dependencies = serde_json::Map::new();
    for entry in fs::read_dir(source.join("ExternalRevisions"))? {
        let entry = entry?;
        if entry.path().is_file() {
            dependencies.insert(
                entry.file_name().to_string_lossy().into_owned(),
                json!(fs::read_to_string(entry.path())?.trim()),
            );
        }
    }
    let mut manifest = baseline();
    manifest.as_object_mut().unwrap().extend(
        json!({
            "version": VERSION,
            "xcode": output(Command::new("xcodebuild").arg("-version"))?,
            "sdk": output(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-version"]))?,
            "settings": fs::read_to_string(config)?,
            "dependencies": dependencies,
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    fs::write(
        destination.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    validate(destination)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let target = destination.join(entry.path().strip_prefix(source)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else {
            ensure!(
                entry.file_type().is_file(),
                "unsupported artifact entry: {}",
                entry.path().display()
            );
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Prepare an artifact from source, a local directory, or a verified download.
/// Explicit source mode takes precedence over artifact environment variables.
pub fn prepare(out: &Path, source: bool) -> Result<PathBuf> {
    fs::create_dir_all(out)?;
    let out = fs::canonicalize(out)?;
    let staged = tempfile::tempdir_in(&out)?;
    if source {
        build(&out.join("source"), staged.path())?;
    } else if let Some(local) = env::var_os("MOLTENVK_STATIC_DIR") {
        let local = Path::new(&local);
        validate(local)?;
        copy_tree(local, staged.path())?;
    } else {
        let missing = "no release is configured yet: set MOLTENVK_STATIC_DIR, set MOLTENVK_ARTIFACT_URL and MOLTENVK_ARTIFACT_SHA256, or enable build-moltenvk";
        let url = env::var("MOLTENVK_ARTIFACT_URL").context(missing)?;
        let checksum = env::var("MOLTENVK_ARTIFACT_SHA256").context(missing)?;
        download(&url, &checksum, &out, staged.path())?;
    }
    let destination = out.join("artifact");
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(staged.path(), &destination)?;
    Ok(destination)
}

/// Write a release archive and its SHA-256 sidecar from a prepared artifact.
pub fn package(artifact: &Path, out: &Path) -> Result<PathBuf> {
    validate(artifact)?;
    fs::create_dir_all(out)?;
    let archive = out.join(ARTIFACT);
    let mut bundle = tar::Builder::new(GzEncoder::new(
        File::create(&archive)?,
        Compression::default(),
    ));
    for entry in WalkDir::new(artifact).sort_by_file_name() {
        let entry = entry?;
        if entry.file_type().is_dir() {
            continue;
        }
        ensure!(
            entry.file_type().is_file(),
            "unsupported artifact entry: {}",
            entry.path().display()
        );
        bundle.append_path_with_name(entry.path(), entry.path().strip_prefix(artifact)?)?;
    }
    bundle.into_inner()?.finish()?;
    fs::write(
        out.join(format!("{ARTIFACT}.sha256")),
        format!("{}  {ARTIFACT}\n", digest(&archive)?),
    )?;
    Ok(archive)
}

#[cfg(test)]
mod tests;
