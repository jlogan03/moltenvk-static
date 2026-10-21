#!/usr/bin/env python3
"""Build, package, or acquire the one supported MoltenVK artifact."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

VERSION = "1.4.2"
REVISION = "db66022459ffb663aa2b50f6b018bc2e124f5edf"
BASELINE = {
    "schema": 1,
    "revision": REVISION,
    "target": "aarch64-apple-darwin",
    "deployment_target": "14.0",
}
ARTIFACT = f"moltenvk-{VERSION}-aarch64-apple-darwin-macos14.tar.gz"


def run(*args, **kwargs):
    subprocess.run(args, check=True, **kwargs)


def output(*args, **kwargs):
    return subprocess.check_output(args, text=True, **kwargs).strip()


def validate(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    for key, value in BASELINE.items():
        if manifest.get(key) != value:
            raise ValueError(f"artifact {key} must be {value!r}")
    with (directory / "libMoltenVK.a").open("rb") as archive:
        if archive.read(8) != b"!<arch>\n":
            raise ValueError("libMoltenVK.a must be a thin static archive")
    if not (directory / "licenses").is_dir():
        raise ValueError("artifact is missing licenses")


def unpack(archive, destination):
    # Extract regular files only. Never follow links or accept parent paths.
    with tarfile.open(archive, "r:gz") as bundle:
        for member in bundle:
            path = Path(member.name)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError(f"invalid archive path: {member.name}")
            target = destination / path
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                target.parent.mkdir(parents=True, exist_ok=True)
                with bundle.extractfile(member) as src, target.open("wb") as dst:
                    shutil.copyfileobj(src, dst)
            else:
                raise ValueError(f"unsupported archive entry: {member.name}")
    validate(destination)


def download(url, checksum, destination):
    if len(checksum) != 64 or any(c not in "0123456789abcdef" for c in checksum):
        raise ValueError("MOLTENVK_ARTIFACT_SHA256 must be a lowercase SHA-256 digest")
    if not url.startswith("https://"):
        raise ValueError("artifact downloads require HTTPS")
    cached = destination.parent / f"{checksum}.tar.gz"
    if not cached.exists():
        temporary = cached.with_suffix(".partial")
        with urllib.request.urlopen(url, timeout=120) as src, temporary.open("wb") as dst:
            if not src.url.startswith("https://"):
                raise ValueError("artifact redirect must use HTTPS")
            shutil.copyfileobj(src, dst)
        temporary.replace(cached)
    if hashlib.sha256(cached.read_bytes()).hexdigest() != checksum:
        cached.unlink()
        raise ValueError("artifact checksum mismatch")
    unpack(cached, destination)


def build(work, destination):
    if platform.system() != "Darwin":
        raise RuntimeError("source builds require macOS with Xcode")
    work.mkdir(parents=True, exist_ok=True)
    source = work / "MoltenVK"
    if not source.exists():
        run("git", "clone", "--depth", "1", "--branch", f"v{VERSION}",
            "https://github.com/KhronosGroup/MoltenVK.git", str(source))
    if output("git", "rev-parse", "HEAD", cwd=source) != REVISION:
        raise ValueError("MoltenVK checkout does not match the pinned revision")
    # Apply the same settings to upstream's nested dependency builds.
    config = work / "build.xcconfig"
    config.write_text("ARCHS = arm64\nONLY_ACTIVE_ARCH = NO\n"
                      "MACOSX_DEPLOYMENT_TARGET = 14.0\n"
                      "CODE_SIGNING_ALLOWED = NO\nLLVM_LTO = NO\n"
                      "GCC_SYMBOLS_PRIVATE_EXTERN = YES\n"
                      "GCC_PREPROCESSOR_DEFINITIONS = $(inherited) MVK_HIDE_VULKAN_SYMBOLS=1\n")
    env = dict(os.environ, XCODE_XCCONFIG_FILE=str(config))
    run("./fetchDependencies", "--macos", cwd=source, env=env)
    run("xcodebuild", "build", "-project", "MoltenVKPackaging.xcodeproj",
        "-scheme", "MoltenVK Package (macOS only)", "-configuration", "Release",
        "-destination", "generic/platform=macOS", cwd=source, env=env)
    libraries = list((source / "Package/Release/MoltenVK/static/MoltenVK.xcframework")
                     .glob("macos-*/libMoltenVK.a"))
    if len(libraries) != 1:
        raise ValueError(f"expected one macOS static library, found {libraries}")
    architectures = output("xcrun", "lipo", "-archs", str(libraries[0])).split()
    if architectures != ["arm64"]:
        raise ValueError(f"expected arm64 only, found {architectures}")
    shutil.copy2(libraries[0], destination / "libMoltenVK.a")
    licenses = destination / "licenses"
    licenses.mkdir()
    # Preserve notices from MoltenVK and its pinned source dependencies.
    for root in [source, *sorted((source / "External").iterdir())]:
        if root.is_dir():
            for pattern in ("LICENSE*", "COPYING*", "NOTICE*"):
                for notice in root.glob(pattern):
                    if notice.is_file():
                        folder = licenses / root.name
                        folder.mkdir(exist_ok=True)
                        shutil.copy2(notice, folder / notice.name)
    manifest = dict(BASELINE, version=VERSION,
                    xcode=output("xcodebuild", "-version"),
                    sdk=output("xcrun", "--sdk", "macosx", "--show-sdk-version"),
                    settings=config.read_text(),
                    dependencies={p.name: p.read_text().strip()
                                  for p in sorted((source / "ExternalRevisions").glob("*"))
                                  if p.is_file()})
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    validate(destination)


def prepare(out, source=False):
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=out) as temporary:
        staged = Path(temporary)
        if source:
            # Explicit source mode always wins over a local or remote artifact.
            build(out / "source", staged)
        elif local := os.environ.get("MOLTENVK_STATIC_DIR"):
            validate(Path(local))
            shutil.copytree(local, staged, dirs_exist_ok=True)
        else:
            url = os.environ.get("MOLTENVK_ARTIFACT_URL")
            checksum = os.environ.get("MOLTENVK_ARTIFACT_SHA256")
            if not url or not checksum:
                raise RuntimeError("no release is configured yet: set MOLTENVK_STATIC_DIR, "
                                   "set MOLTENVK_ARTIFACT_URL and MOLTENVK_ARTIFACT_SHA256, "
                                   "or enable build-moltenvk")
            download(url, checksum, staged)
        destination = out / "artifact"
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(staged, destination)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "package"])
    parser.add_argument("output", type=Path)
    parser.add_argument("--source", action="store_true")
    args = parser.parse_args()
    out = args.output.resolve()
    artifact = prepare(out, source=args.source or args.command == "package")
    if args.command == "package":
        archive = out / ARTIFACT
        with tarfile.open(archive, "w:gz") as bundle:
            for path in sorted(artifact.rglob("*")):
                if path.is_file():
                    bundle.add(path, arcname=str(path.relative_to(artifact)), recursive=False)
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        archive.with_name(archive.name + ".sha256").write_text(f"{checksum}  {archive.name}\n")
        print(archive)


if __name__ == "__main__":
    main()
