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

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub(crate) struct SecretString(Zeroizing<String>);

impl SecretString {
    pub(crate) fn new(secret: String) -> Self {
        Self(Zeroizing::new(secret))
    }

    pub(crate) fn get_secret_value(&self) -> &str {
        self.0.as_ref()
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SecretString").field(&"***").finish()
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self(Zeroizing::new(String::deserialize(deserializer)?)))
    }
}

pub(crate) struct SecretStringCollection(Zeroizing<Vec<String>>);

impl SecretStringCollection {
    pub(crate) fn new() -> Self {
        Self(Zeroizing::new(Vec::new()))
    }

    pub(crate) fn push(&mut self, secret: String) {
        if secret.is_empty() {
            return;
        }
        let index = match self
            .0
            .binary_search_by(|probe| secret.len().cmp(&probe.len()))
        {
            Ok(index) => index,
            Err(index) => index,
        };
        self.0.insert(index, secret);
    }

    pub(crate) fn anonymize<'a>(&self, string: &'a str) -> Cow<'a, str> {
        let mut string = Cow::Borrowed(string);
        for secret in self.0.iter() {
            let mut output: Option<String> = None;
            let input = string.as_ref();
            for (index, _) in input.rmatch_indices(secret) {
                let mut new_output = String::new();
                let new_input = output.as_deref().unwrap_or(input);
                new_output.push_str(&new_input[..index]);
                new_output.push_str("***");
                new_output.push_str(&new_input[index + secret.len()..]);
                output = Some(new_output);
            }
            if let Some(output) = output.take() {
                string = Cow::Owned(output);
            }
        }
        string
    }
}

impl std::fmt::Debug for SecretStringCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|_| "***"))
            .finish()
    }
}

#[cfg(test)]
mod test_secret_string_collection {
    use super::SecretStringCollection;

    #[test]
    fn test_ordered_by_len_descending() {
        let mut collection = SecretStringCollection::new();

        collection.push("aaaa".into());
        collection.push("bb".into());
        collection.push("ccc".into());
        collection.push("d".into());

        assert_eq!(collection.0.as_ref(), ["aaaa", "ccc", "bb", "d"]);
    }

    #[test]
    fn test_ignore_empty_secrets() {
        let mut collection = SecretStringCollection::new();

        collection.push("SECRET".into());
        collection.push("".into());

        assert_eq!(collection.0.as_ref(), ["SECRET"]);
    }

    #[test]
    fn test_anonymize() {
        let mut collection = SecretStringCollection::new();

        collection.push("SECRET".into());
        collection.push("ANOTHER_ONE".into());
        collection.push("MORE_SECRET".into());
        collection.push("SECRET".into());

        assert_eq!(collection.anonymize("input".into()), "input");
        assert_eq!(collection.anonymize("SECRET".into()), "***");
        assert_eq!(
            collection.anonymize("123ANOTHER_ONE456MORE_SECRET789".into()),
            "123***456***789"
        );
    }

    #[test]
    fn test_multiple_matches() {
        let mut collection = SecretStringCollection::new();

        collection.push("aba".into());

        assert_eq!(
            collection.anonymize("123aba123ababa123".into()),
            "123***123ab***123"
        );
    }
}
