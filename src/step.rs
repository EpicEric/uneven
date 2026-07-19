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
        let mut secrets: SecretStringCollection = SecretStringCollection::new();

        for (_, value) in env {
            let UnevenStepEnvVar::Secret(secret) = value else {
                continue;
            };
            let Some(secret) = self.secrets.get(&secret.secret_name) else {
                return Err(color_eyre::eyre::eyre!(
                    "Unknown secret {}",
                    secret.secret_name
                ));
            };
            secrets.push(secret.get_secret_value().to_string());
        }

        let mut command = Command::new(&derivation);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
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
