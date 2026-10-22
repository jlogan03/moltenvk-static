use super::*;

fn fixture(root: &Path) -> Result<PathBuf> {
    let artifact = root.join("fixture");
    fs::create_dir_all(artifact.join("licenses/MoltenVK"))?;
    fs::write(artifact.join("manifest.json"), baseline().to_string())?;
    // An empty ar archive tests packaging without native object files.
    fs::write(artifact.join("libMoltenVK.a"), b"!<arch>\n")?;
    fs::write(artifact.join("licenses/MoltenVK/LICENSE"), "test fixture")?;
    Ok(artifact)
}

#[test]
fn package_round_trip_through_verified_cache() -> Result<()> {
    let root = tempfile::tempdir()?;
    let archive = package(&fixture(root.path())?, root.path())?;
    let checksum = digest(&archive)?;
    assert_eq!(
        fs::read_to_string(root.path().join(format!("{ARTIFACT}.sha256")))?,
        format!("{checksum}  {ARTIFACT}\n")
    );
    fs::rename(archive, root.path().join(format!("{checksum}.tar.gz")))?;
    let destination = root.path().join("output");
    download(
        "https://unused.invalid/artifact",
        &checksum,
        root.path(),
        &destination,
    )?;
    validate(&destination)?;
    assert_eq!(
        fs::read_to_string(destination.join("licenses/MoltenVK/LICENSE"))?,
        "test fixture"
    );
    Ok(())
}

#[test]
fn corrupt_cache_is_rejected_before_extraction() -> Result<()> {
    let root = tempfile::tempdir()?;
    let checksum = "0".repeat(64);
    let cached = root.path().join(format!("{checksum}.tar.gz"));
    fs::write(&cached, b"corrupt")?;
    let destination = root.path().join("output");
    let error = download(
        "https://unused.invalid/artifact",
        &checksum,
        root.path(),
        &destination,
    )
    .unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"));
    assert!(!destination.exists());
    assert!(!cached.exists());
    Ok(())
}

#[test]
fn paths_and_links_cannot_escape() -> Result<()> {
    for (name, kind) in [
        ("../escape", tar::EntryType::Regular),
        ("/escape", tar::EntryType::Regular),
        ("link", tar::EntryType::Symlink),
        ("link", tar::EntryType::Link),
    ] {
        let root = tempfile::tempdir()?;
        let archive = root.path().join("input.tar.gz");
        let mut bundle = tar::Builder::new(GzEncoder::new(
            File::create(&archive)?,
            Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        // Set raw name bytes because tar's builder rejects unsafe paths.
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(kind);
        header.set_mode(0o644);
        header.set_size(0);
        if kind.is_symlink() || kind.is_hard_link() {
            header.set_link_name("../escape")?;
        }
        header.set_cksum();
        bundle.append(&header, io::empty())?;
        bundle.into_inner()?.finish()?;
        let error = unpack(&archive, &root.path().join("output")).unwrap_err();
        assert!(
            error.to_string().contains(if kind.is_file() {
                "invalid archive path"
            } else {
                "unsupported archive entry"
            }),
            "{error:#}"
        );
        assert!(!root.path().join("escape").exists());
    }
    Ok(())
}

#[test]
fn incompatible_artifact_metadata_is_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let artifact = fixture(root.path())?;
    for field in ["revision", "patches"] {
        let mut manifest = baseline();
        manifest.as_object_mut().unwrap().remove(field);
        fs::write(artifact.join("manifest.json"), manifest.to_string())?;
        assert!(validate(&artifact).unwrap_err().to_string().contains(field));
    }
    Ok(())
}

#[test]
fn invalid_download_settings_are_rejected_without_network_access() -> Result<()> {
    let root = tempfile::tempdir()?;
    for (url, checksum, message) in [
        ("http://unused.invalid/artifact", "0".repeat(64), "HTTPS"),
        (
            "https://unused.invalid/artifact",
            "invalid".into(),
            "SHA-256",
        ),
    ] {
        let error = download(url, &checksum, root.path(), &root.path().join("output")).unwrap_err();
        assert!(error.to_string().contains(message));
    }
    Ok(())
}
