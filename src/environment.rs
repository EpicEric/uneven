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
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use color_eyre::eyre::OptionExt;
use serde::{Deserialize, Serialize};

use crate::secret::SecretString;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct UnevenEnvironment {
    pub(crate) secrets: HashMap<String, SecretString>,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) uploads: HashMap<String, PathBuf>,
}

impl UnevenEnvironment {
    pub(crate) fn get() -> color_eyre::Result<UnevenEnvironment> {
        #[derive(Debug, Deserialize)]
        struct UnevenEnvironmentInit {
            pub(crate) secrets: Vec<String>,
            pub(crate) vars: HashMap<String, String>,
        }

        let mut env_vars: HashMap<OsString, OsString> = std::env::vars_os().collect();

        let env: UnevenEnvironmentInit =
            match env_vars.remove(OsStr::from_bytes("UNEVEN_ENVIRONMENT".as_bytes())) {
                Some(value) => serde_json::from_slice(value.as_bytes())?,
                None => return Err(color_eyre::eyre::eyre!("Missing UNEVEN_ENVIRONMENT envvar")),
            };

        let secrets: color_eyre::Result<HashMap<String, SecretString>> = env
            .secrets
            .into_iter()
            .map(
                |secret| match env_vars.remove(OsStr::from_bytes(secret.as_bytes())) {
                    Some(value) => {
                        let value = SecretString::new(value.into_string().map_err(|_| {
                            color_eyre::eyre::eyre!("Invalid value for {secret} envvar")
                        })?);
                        Ok((secret, value))
                    }
                    None => Err(color_eyre::eyre::eyre!("Missing {secret} envvar")),
                },
            )
            .collect();

        Ok(Self {
            secrets: secrets?,
            vars: env.vars,
            uploads: HashMap::new(),
        })
    }

    pub(crate) fn upload(&mut self, name: String, derivation: PathBuf) -> color_eyre::Result<()> {
        if self.uploads.insert(name, derivation).is_some() {
            Err(color_eyre::eyre::eyre!("Upload key already used"))
        } else {
            Ok(())
        }
    }

    pub(crate) fn download(&self, name: &str) -> color_eyre::Result<&Path> {
        self.uploads
            .get(name)
            .map(|path| path.as_ref())
            .ok_or_eyre("Missing upload key")
    }
}
