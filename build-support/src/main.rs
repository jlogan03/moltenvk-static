use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = env::args_os().skip(1);
    let out = args.next().map(PathBuf::from);
    anyhow::ensure!(
        out.is_some() && args.next().is_none(),
        "usage: cargo run -p moltenvk-build -- <output-directory>"
    );
    let out = out.unwrap();
    let artifact = moltenvk_build::prepare(&out, true)?;
    println!("{}", moltenvk_build::package(&artifact, &out)?.display());
    Ok(())
}
