// uneven: A Nix-based distributed command runner
// Copyright (C) 2026 Eric Rodrigues Pires
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for
// more details.
//
// You should have received a copy of the GNU Affero General Public License along
// with this program. If not, see <https://www.gnu.org/licenses/>.

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
