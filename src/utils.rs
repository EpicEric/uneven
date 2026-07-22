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
    ffi::OsString,
    io::Write,
    os::unix::ffi::{OsStrExt, OsStringExt},
};

use smol::{io::AsyncReadExt, process::Child};

pub(crate) fn escape_os_string(string: OsString) -> OsString {
    if !string.is_empty()
        && string.as_bytes().iter().all(|byte| {
            matches!(byte, b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'='
                | b'/'
                | b','
                | b'.'
                | b'+')
        })
    {
        return string;
    }

    let mut escaped = Vec::new();
    escaped.push(b'\'');
    for char in string.as_encoded_bytes() {
        match char {
            b'\'' | b'!' => {
                escaped.extend(b"'\\");
                escaped.push(*char);
                escaped.push(b'\'');
            }
            _ => escaped.push(*char),
        }
    }
    escaped.push(b'\'');

    OsString::from_vec(escaped)
}

pub(crate) async fn pipe_outputs_to_stderr(child: &mut Child) -> color_eyre::Result<()> {
    let mut stderr = std::io::stderr();
    if let Some(mut pipe) = child.stdout.take() {
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf).await?;
        stderr.write_all(&buf)?;
    }
    if let Some(mut pipe) = child.stderr.take() {
        let mut buf = Vec::new();
        pipe.read_to_end(&mut buf).await?;
        stderr.write_all(&buf)?;
    }
    Ok(stderr.flush()?)
}
