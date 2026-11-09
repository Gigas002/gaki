use sha1::{Digest, Sha1};

/// The machine ID Steam asks for in login device details: three SHA-1
/// values (`BB3`, `FF2`, `3B3`), sent as a binary KeyValues "MessageObject".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineId {
    /// The `BB3` value.
    pub bb3: [u8; 20],
    /// The `FF2` value.
    pub ff2: [u8; 20],
    /// The `3B3` value.
    pub value_3b3: [u8; 20],
}

impl MachineId {
    /// A stable ID derived from `identifier` (one-way, but repeatable), the
    /// way the official client derives one from the account name.
    pub fn from_identifier(identifier: &str) -> Self {
        let hash = |key: &str| -> [u8; 20] {
            Sha1::digest(format!("SteamUser Hash {key} {identifier}")).into()
        };
        Self {
            bb3: hash("BB3"),
            ff2: hash("FF2"),
            value_3b3: hash("3B3"),
        }
    }

    /// A random ID.
    pub fn random() -> Self {
        Self {
            bb3: rand::random(),
            ff2: rand::random(),
            value_3b3: rand::random(),
        }
    }

    /// The binary KeyValues message object Steam expects (155 bytes): each
    /// value as a NUL-terminated uppercase hex string.
    pub fn to_message(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(155);
        out.push(0); // object start
        out.extend_from_slice(b"MessageObject\0");
        for (key, value) in [
            (b"BB3\0", &self.bb3),
            (b"FF2\0", &self.ff2),
            (b"3B3\0", &self.value_3b3),
        ] {
            out.push(1); // string entry
            out.extend_from_slice(key);
            out.extend_from_slice(hex::encode_upper(value).as_bytes());
            out.push(0);
        }
        out.extend_from_slice(&[8, 8]); // end of object, end of document
        out
    }
}

#[cfg(test)]
mod tests;
