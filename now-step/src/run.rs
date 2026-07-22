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

use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    io::{BufRead, BufReader},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    thread::spawn,
};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::secrets::SecretStringCollection;

pub(crate) fn run(derivation: PathBuf, secrets: &Vec<String>) -> color_eyre::Result<()> {
    let env: HashMap<OsString, OsString> = std::env::vars_os().collect();

    let mut secret_collection: SecretStringCollection = SecretStringCollection::new();

    for secret in secrets {
        let Some(secret_value) = env.get(OsStr::from_bytes(secret.as_bytes())) else {
            return Err(color_eyre::eyre::eyre!("Unknown secret {}", secret));
        };
        secret_collection.push(
            secret_value
                .to_str()
                .ok_or_else(|| color_eyre::eyre::eyre!("Invalid value for {secret} envvar"))?
                .to_string(),
        );
    }

    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| color_eyre::eyre::eyre!("{error}"))?;

    let mut command = CommandBuilder::new(&derivation);
    for (key, value) in env {
        command.env(key, value);
    }
    command.env("CI", "1");
    command.env("NO_COLOR", "1");
    command.cwd(std::env::current_dir()?);
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| color_eyre::eyre::eyre!("{error}"))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| color_eyre::eyre::eyre!("{error}"))?;
    drop(
        pair.master
            .take_writer()
            .expect("writer has not been taken"),
    );

    let jh = spawn(move || {
        let mut lines = BufReader::new(reader).lines();
        let _ = lines.next(); // First line is always empty
        for line in lines {
            if let Ok(line) = line {
                eprintln!("{}", secret_collection.anonymize(&line));
            } else {
                break;
            }
        }
    });

    let status = child.wait()?;
    drop(pair);
    jh.join().expect("no panic in join handle");

    if status.success() {
        Ok(())
    } else {
        Err(color_eyre::eyre::eyre!("{}", status))
    }
}
