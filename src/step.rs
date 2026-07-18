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
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread::scope,
};

use crate::{
    environment::UnevenEnvironment, secret::SecretStringCollection, workflow::UnevenStepEnvVar,
};

impl UnevenEnvironment {
    pub(crate) fn run_step(
        &self,
        derivation: PathBuf,
        teardown: bool,
        env: &HashMap<String, UnevenStepEnvVar>,
    ) -> color_eyre::Result<()> {
        let mut step_env: HashMap<OsString, OsString> = HashMap::new();
        let mut secrets: SecretStringCollection = SecretStringCollection::new();

        for (key, value) in env {
            let value = match value {
                UnevenStepEnvVar::Plain(value) => value.into(),
                UnevenStepEnvVar::Secret(secret) => {
                    let Some(secret) = self.secrets.get(&secret.secret_name) else {
                        return Err(color_eyre::eyre::eyre!(
                            "Unknown secret {}",
                            secret.secret_name
                        ));
                    };
                    let secret = secret.get_secret_value();
                    secrets.push(secret.to_string());
                    secret.into()
                }
            };
            step_env.insert(key.into(), value);
        }

        let mut command = Command::new(&derivation);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        if let Some((_, path)) = std::env::vars_os().find(|(name, _)| name == "PATH") {
            command.env("PATH", path);
        }
        if let Some((_, path)) = std::env::vars_os().find(|(name, _)| name == "HOME") {
            command.env("HOME", path);
        }
        command.envs(&step_env);
        let mut child = command.spawn()?;

        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        let secrets = &secrets;
        let result: color_eyre::Result<()> = scope(move |s| {
            let stdout_task = s.spawn::<_, color_eyre::Result<()>>(move || {
                let mut parent_stdout = io::stdout();
                for line in BufReader::new(stdout).lines() {
                    parent_stdout.write_all(secrets.anonymize(&line?).as_bytes())?;
                }
                Ok(())
            });
            let stderr_task = s.spawn::<_, color_eyre::Result<()>>(move || {
                let mut parent_stderr = io::stderr();
                for line in BufReader::new(stderr).lines() {
                    parent_stderr.write_all(secrets.anonymize(&line?).as_bytes())?;
                }
                Ok(())
            });
            stdout_task.join().expect("no panic in stdout task")?;
            stderr_task.join().expect("no panic in stderr task")?;
            Ok(())
        });

        if teardown { Ok(()) } else { result }
    }
}
