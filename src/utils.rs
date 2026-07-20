use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
};

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
