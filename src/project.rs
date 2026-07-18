use std::{env::temp_dir, fs::create_dir_all, path::PathBuf};

use include_dir::{Dir, include_dir};

static CARGO_TOML: &[u8] = include_bytes!("../Cargo.toml");
static CARGO_LOCK: &[u8] = include_bytes!("../Cargo.lock");
static NIX_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/nix");
static SRC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/src");

pub(crate) fn create_project_source() -> color_eyre::Result<PathBuf> {
    let mut tmpdir = temp_dir();
    tmpdir.push(format!("uneven-{}", uuid::Uuid::new_v4()));

    let mut nix_dir = tmpdir.clone();
    nix_dir.push("nix");
    create_dir_all(&nix_dir)?;
    NIX_DIR.extract(&nix_dir)?;

    let mut src_dir = tmpdir.clone();
    src_dir.push("src");
    create_dir_all(&src_dir)?;
    SRC_DIR.extract(&src_dir)?;

    let mut cargo_toml = tmpdir.clone();
    cargo_toml.push("Cargo.toml");
    std::fs::write(cargo_toml, CARGO_TOML)?;

    let mut cargo_lock = tmpdir.clone();
    cargo_lock.push("Cargo.lock");
    std::fs::write(cargo_lock, CARGO_LOCK)?;

    Ok(tmpdir)
}
