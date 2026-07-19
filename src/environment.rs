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

use serde::{Deserialize, Serialize};

use crate::{secret::SecretString, workflow::UnevenStepEnvVar};

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct UnevenEnvironment {
    pub(crate) secrets: HashMap<String, SecretString>,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) uploads: HashMap<String, PathBuf>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct UnevenEnvironmentInit {
    #[serde(default)]
    pub(crate) secrets: Vec<String>,
    #[serde(default)]
    pub(crate) vars: HashMap<String, String>,
    #[serde(default)]
    pub(crate) uploads: HashMap<String, PathBuf>,
}

static UNEVEN_ENVIRONMENT_KEY: &str = "UNEVEN_ENVIRONMENT";

impl UnevenEnvironment {
    pub(crate) fn get() -> color_eyre::Result<UnevenEnvironment> {
        let mut env_vars: HashMap<OsString, OsString> = std::env::vars_os().collect();

        let env: UnevenEnvironmentInit =
            match env_vars.remove(OsStr::from_bytes(UNEVEN_ENVIRONMENT_KEY.as_bytes())) {
                Some(value) => serde_json::from_slice(value.as_bytes())?,
                None => return Ok(Default::default()),
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
            uploads: env.uploads,
        })
    }

    pub(crate) fn env_vars(
        &self,
        step_env: &HashMap<String, UnevenStepEnvVar>,
    ) -> color_eyre::Result<impl Iterator<Item = (OsString, OsString)>> {
        let mut env_init = UnevenEnvironmentInit {
            uploads: self.uploads.clone(),
            ..Default::default()
        };

        let mut map: HashMap<OsString, OsString> = HashMap::with_capacity(step_env.len() + 1);

        for (key, value) in step_env {
            match value {
                UnevenStepEnvVar::Plain(value) => {
                    env_init.vars.insert(key.clone(), value.clone());
                }
                UnevenStepEnvVar::Secret(secret) => {
                    map.insert(
                        key.into(),
                        self.secrets
                            .get(&secret.secret_name)
                            .ok_or_else(|| {
                                color_eyre::eyre::eyre!("Missing secret {}", &secret.secret_name)
                            })?
                            .get_secret_value()
                            .into(),
                    );
                    env_init.secrets.push(secret.secret_name.clone());
                }
                UnevenStepEnvVar::Download(download) => {
                    let download_path =
                        self.uploads.get(&download.download_name).ok_or_else(|| {
                            color_eyre::eyre::eyre!("Missing download {}", &download.download_name)
                        })?;
                    map.insert(key.into(), download_path.into());
                    env_init.vars.insert(
                        key.into(),
                        download_path
                            .to_str()
                            .ok_or_else(|| {
                                color_eyre::eyre::eyre!(
                                    "Invalid UTF-8 for download path of {}",
                                    &download.download_name
                                )
                            })?
                            .into(),
                    );
                }
            }
        }

        map.insert(
            UNEVEN_ENVIRONMENT_KEY.into(),
            serde_json::to_string(&env_init)?.into(),
        );

        Ok(map.into_iter())
    }

    pub(crate) fn download(&self, name: &str) -> color_eyre::Result<&Path> {
        self.uploads
            .get(name)
            .map(|path| path.as_ref())
            .ok_or_else(|| color_eyre::eyre::eyre!("Missing upload key '{name}'"))
    }
}
