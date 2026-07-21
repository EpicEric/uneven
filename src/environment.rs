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
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    io::Write,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

use crate::{
    secret::SecretString,
    workflow::{UnevenJob, UnevenJobContainer, UnevenStepEnvVar, UnevenWorkflow},
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct UnevenEnvironment {
    pub(crate) secrets: HashMap<String, SecretString>,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) local_env: HashMap<OsString, OsString>,
    pub(crate) uploads: Mutex<HashMap<String, PathBuf>>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct UnevenEnvironmentInit {
    #[serde(default)]
    pub(crate) secrets: HashMap<String, String>,
    #[serde(default)]
    pub(crate) vars: HashMap<String, String>,
    #[serde(default)]
    pub(crate) uploads: HashMap<String, PathBuf>,
}

static UNEVEN_ENVIRONMENT_KEY: &str = "UNEVEN_ENVIRONMENT";

struct ParsedWorkflow {
    vars: HashSet<String>,
    secrets: HashSet<String>,
}

impl UnevenEnvironment {
    pub(crate) fn get_for_workflow(
        workflow: &Path,
        env_file: Option<&PathBuf>,
    ) -> color_eyre::Result<UnevenEnvironment> {
        let mut env_vars: HashMap<OsString, OsString> = HashMap::new();
        if let Some(env_file) = env_file {
            env_vars.extend(
                dotenvy::from_path_iter(env_file)?.filter_map(|result| {
                    result.ok().map(|(key, value)| (key.into(), value.into()))
                }),
            );
        };
        env_vars.extend(std::env::vars_os());

        let parsed_workflow = Self::parse_workflow(workflow, &env_vars)?;

        let secrets: color_eyre::Result<HashMap<String, SecretString>> = parsed_workflow
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

        let vars: color_eyre::Result<HashMap<String, String>> = parsed_workflow
            .vars
            .into_iter()
            .map(
                |var| match env_vars.remove(OsStr::from_bytes(var.as_bytes())) {
                    Some(value) => {
                        let value = value.into_string().map_err(|_| {
                            color_eyre::eyre::eyre!("Invalid value for {var} envvar")
                        })?;
                        Ok((var, value))
                    }
                    None => Err(color_eyre::eyre::eyre!("Missing {var} envvar")),
                },
            )
            .collect();

        Ok(Self {
            secrets: secrets?,
            vars: vars?,
            local_env: env_vars,
            uploads: Default::default(),
        })
    }

    fn parse_workflow(
        workflow: &Path,
        env_vars: &HashMap<OsString, OsString>,
    ) -> color_eyre::Result<ParsedWorkflow> {
        let workflow_canonical = std::fs::canonicalize(workflow)?;
        let workflow_str = workflow_canonical
            .to_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("non-UTF8 path"))?;
        let workflow_path = format!("(/. + {})", serde_json::to_string(&workflow_str)?);

        let env_var_names = serde_json::to_string(&serde_json::to_string(
            &env_vars
                .keys()
                .filter_map(|key| key.to_str())
                .collect::<Vec<_>>(),
        )?)?;

        let nix_command = format!(
            "import ./nix/env.nix {{ }} {workflow_path} (builtins.fromJSON {env_var_names})"
        );

        let mut command = Command::new("nix");
        command.args([
            "--extra-experimental-features",
            "nix-command",
            "eval",
            "--impure",
            "--json",
        ]);
        let output = command.arg("--expr").arg(nix_command).output()?;

        if !output.status.success() {
            let mut stderr = std::io::stderr();
            stderr.write_all(&output.stderr)?;
            stderr.flush()?;
            return Err(color_eyre::eyre::eyre!(
                "Failed to parse workflow for variables"
            ));
        }

        let workflow: UnevenWorkflow = serde_json::from_slice(&output.stdout)?;

        let mut vars: HashSet<String> = HashSet::new();
        let mut secrets: HashSet<String> = HashSet::new();

        let vars_regex = regex::Regex::new(r#"@@__unevenVar_([^@]+)@@"#).expect("valid regex");

        let mut job_fn = |job: &UnevenJob| {
            for step in &job.steps {
                for env_value in step.env.values() {
                    match env_value {
                        UnevenStepEnvVar::Plain(var) => {
                            vars.extend(vars_regex.captures_iter(var).map(|needle| {
                                needle.get(1).expect("is match").as_str().to_string()
                            }));
                        }
                        UnevenStepEnvVar::Secret(secret) => {
                            secrets.insert(secret.secret_name.clone());
                        }
                        UnevenStepEnvVar::Download(_) => {}
                    }
                }
            }
        };

        for job in workflow.jobs.values() {
            match job {
                UnevenJobContainer::Single(job) => (job_fn)(job),
                UnevenJobContainer::Multiple(job_vec) => {
                    for job in job_vec {
                        (job_fn)(job)
                    }
                }
            }
        }

        for secret in &secrets {
            if vars.contains(secret) {
                return Err(color_eyre::eyre::eyre!(
                    "Secret '{secret}' cannot also be used as a regular variable"
                ));
            }
        }

        Ok(ParsedWorkflow { vars, secrets })
    }

    pub(crate) fn get_for_step() -> color_eyre::Result<UnevenEnvironment> {
        let mut env_vars: HashMap<OsString, OsString> = std::env::vars_os().collect();

        let env: UnevenEnvironmentInit =
            match env_vars.remove(OsStr::from_bytes(UNEVEN_ENVIRONMENT_KEY.as_bytes())) {
                Some(value) => serde_json::from_slice(value.as_bytes())?,
                None => {
                    return Err(color_eyre::eyre::eyre!(
                        "Missing {UNEVEN_ENVIRONMENT_KEY} envvar"
                    ));
                }
            };

        let secrets: color_eyre::Result<HashMap<String, SecretString>> = env
            .secrets
            .into_iter()
            .map(
                |(env_var, secret)| match env_vars.remove(OsStr::from_bytes(env_var.as_bytes())) {
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
            local_env: HashMap::new(),
            uploads: Mutex::new(env.uploads),
        })
    }

    pub(crate) fn generate_env_vars_for_step(
        &self,
        step_env: &HashMap<String, UnevenStepEnvVar>,
    ) -> color_eyre::Result<HashMap<OsString, OsString>> {
        let mut env_init = UnevenEnvironmentInit {
            uploads: self.uploads.lock().expect("not poisoned").clone(),
            ..Default::default()
        };

        let mut map: HashMap<OsString, OsString> = HashMap::with_capacity(step_env.len() + 1);

        {
            let uploads = self.uploads.lock().expect("not poisoned");
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
                                    color_eyre::eyre::eyre!(
                                        "Missing secret {}",
                                        &secret.secret_name
                                    )
                                })?
                                .get_secret_value()
                                .into(),
                        );
                        env_init
                            .secrets
                            .insert(key.clone(), secret.secret_name.clone());
                    }
                    UnevenStepEnvVar::Download(download) => {
                        let download_path =
                            uploads.get(&download.download_name).ok_or_else(|| {
                                color_eyre::eyre::eyre!(
                                    "Missing download {}",
                                    &download.download_name
                                )
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
        }

        map.insert(
            UNEVEN_ENVIRONMENT_KEY.into(),
            serde_json::to_string(&env_init)?.into(),
        );

        Ok(map)
    }
}
