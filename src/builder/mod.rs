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

use std::{
    collections::HashMap,
    ffi::OsString,
    io::PipeReader,
    path::{Path, PathBuf},
    process::Child,
};

use serde::Deserialize;

use crate::workflow::UnevenJob;

pub(crate) mod local;
pub(crate) mod remote;

pub(crate) trait UnevenBuilder {
    fn get_name(&self) -> String;

    fn get_style(&self) -> owo_colors::Style;

    fn checkout(&self) -> color_eyre::Result<PathBuf>;

    fn copy_derivations(&self, job: &UnevenJob) -> color_eyre::Result<()>;

    fn realize_derivation(&self, derivation: &Path) -> color_eyre::Result<PathBuf>;

    fn download(&self, downloads: &[&Path]) -> color_eyre::Result<()>;

    fn run_derivation(
        &self,
        cwdir: &Path,
        derivation: PathBuf,
        envs: HashMap<OsString, OsString>,
    ) -> color_eyre::Result<(Child, PipeReader)>;

    fn fetch_derivation(&self, derivation: &Path) -> color_eyre::Result<()>;

    fn undo_checkout(&self, path: &Path) -> color_eyre::Result<()>;
}

#[derive(Deserialize, Debug)]
pub(crate) struct NixConfigValue<T> {
    value: T,
}

#[derive(Deserialize, Debug)]
pub(crate) struct NixConfig {
    builders: NixConfigValue<String>,
    system: NixConfigValue<String>,
    #[serde(rename = "system-features")]
    system_features: NixConfigValue<Vec<String>>,
}
