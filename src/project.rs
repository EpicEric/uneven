// now: A Nix-based distributed command runner
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

static NIX_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/nix");
static NOW_STEP_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/now-step");

pub(crate) fn create_nix_project_source() -> color_eyre::Result<PathBuf> {
    let tmpdir = temp_dir().join(format!("now-{}", uuid::Uuid::new_v4()));

    let nix_dir = tmpdir.join("nix");
    create_dir_all(&nix_dir)?;
    NIX_DIR.extract(&nix_dir)?;

    let now_step_dir = tmpdir.join("now-step");
    create_dir_all(&now_step_dir)?;
    NOW_STEP_DIR.extract(&now_step_dir)?;

    Ok(tmpdir)
}
