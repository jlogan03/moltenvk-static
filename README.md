# moltenvk-static

Static MoltenVK packaging for **ARM64 macOS 12 and later**. CI builds on macOS 15.

MoltenVK pin is **1.4.2**, commit
`db66022459ffb663aa2b50f6b018bc2e124f5edf`.
Release artifact:
`moltenvk-1.4.2-aarch64-apple-darwin-macos12.tar.gz`.

The build applies a one-line patch to MoltenVK to register `vkReleaseSwapchainImagesEXT`
against its KHR implementation when Vulkan symbols are hidden.

## Build from source

On a Mac with full Xcode selected, Git, and the tools required by
MoltenVK's dependency build installed:

```sh
cargo run --features build-moltenvk --example probe
```

This downloads MoltenVK source and its dependencies, then builds static archives
with LTO disabled.

System dependencies
* Apple frameworks
* C++ runtime

## Produce a release artifact

```sh
cargo run -p moltenvk-build -- dist
MOLTENVK_STATIC_DIR="$PWD/dist/artifact" cargo run --example probe
```

## Consume an artifact

For a local unpacked artifact, set `MOLTENVK_STATIC_DIR` as above. This path makes
no network requests. Otherwise, configure a release URL and its expected digest:

```sh
export MOLTENVK_ARTIFACT_URL='https://your-release-host/path/to/artifact.tar.gz'
export MOLTENVK_ARTIFACT_SHA256='<64-character SHA-256 digest>'
cargo build
```

Cargo builds and release packaging share the Rust `build-support` crate. Prebuilt
artifacts require no Python installation. Source builds still need the tools
required by MoltenVK's dependency build.

Run the artifact tests with `cargo test -p moltenvk-build`.
