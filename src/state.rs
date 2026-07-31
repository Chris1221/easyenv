use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::collections::BTreeMap;

/// Bumped whenever the wire format changes. `decode` returns `None` on a
/// version mismatch (or any other malformed input) rather than erroring, so
/// an easyenv upgrade mid-session just triggers a safe fresh recompute
/// instead of a hard failure.
const FORMAT_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriorValue {
    /// This key did not exist before easyenv touched it; unset it on unload.
    Unset,
    /// This key had this value before easyenv touched it; restore it.
    Set(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedVar {
    pub prior: PriorValue,
}

/// Everything easyenv needs to remember between shell-hook invocations,
/// round-tripped through a single `EASYENV_STATE` environment variable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EasyenvState {
    pub managed: BTreeMap<String, ManagedVar>,
    pub signature: u64,
}

impl EasyenvState {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Encodes as a version-tagged, length-prefixed byte stream, then
    /// base64s the whole thing into one token safe for a single shell
    /// `export` assignment. Length-prefixing (rather than delimiters) avoids
    /// escaping bugs since `.env` values may contain `\n`, `=`, or quotes.
    pub fn encode(&self) -> String {
        let mut buf = Vec::new();
        buf.push(FORMAT_VERSION);
        buf.extend_from_slice(&self.signature.to_le_bytes());
        buf.extend_from_slice(&(self.managed.len() as u32).to_le_bytes());
        for (key, mv) in &self.managed {
            let kb = key.as_bytes();
            buf.extend_from_slice(&(kb.len() as u32).to_le_bytes());
            buf.extend_from_slice(kb);
            match &mv.prior {
                PriorValue::Unset => buf.push(0),
                PriorValue::Set(v) => {
                    buf.push(1);
                    let vb = v.as_bytes();
                    buf.extend_from_slice(&(vb.len() as u32).to_le_bytes());
                    buf.extend_from_slice(vb);
                }
            }
        }
        STANDARD.encode(buf)
    }

    /// Returns `None` on any malformed/incompatible token; callers should
    /// treat that as `EasyenvState::empty()` (self-healing, never a crash).
    pub fn decode(token: &str) -> Option<Self> {
        let bytes = STANDARD.decode(token).ok()?;
        let mut pos = 0usize;

        let version = read_u8(&bytes, &mut pos)?;
        if version != FORMAT_VERSION {
            return None;
        }
        let signature = read_u64(&bytes, &mut pos)?;
        let count = read_u32(&bytes, &mut pos)? as usize;

        let mut managed = BTreeMap::new();
        for _ in 0..count {
            let keylen = read_u32(&bytes, &mut pos)? as usize;
            let key = String::from_utf8(read_bytes(&bytes, &mut pos, keylen)?).ok()?;
            let tag = read_u8(&bytes, &mut pos)?;
            let prior = match tag {
                0 => PriorValue::Unset,
                1 => {
                    let vallen = read_u32(&bytes, &mut pos)? as usize;
                    let val = String::from_utf8(read_bytes(&bytes, &mut pos, vallen)?).ok()?;
                    PriorValue::Set(val)
                }
                _ => return None,
            };
            managed.insert(key, ManagedVar { prior });
        }
        Some(EasyenvState { managed, signature })
    }
}

fn read_u8(bytes: &[u8], pos: &mut usize) -> Option<u8> {
    let b = *bytes.get(*pos)?;
    *pos += 1;
    Some(b)
}

fn read_u32(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    let slice = bytes.get(*pos..*pos + 4)?;
    *pos += 4;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn read_u64(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let slice = bytes.get(*pos..*pos + 8)?;
    *pos += 8;
    Some(u64::from_le_bytes(slice.try_into().ok()?))
}

fn read_bytes(bytes: &[u8], pos: &mut usize, len: usize) -> Option<Vec<u8>> {
    let slice = bytes.get(*pos..*pos + len)?;
    *pos += len;
    Some(slice.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_empty_state() {
        let state = EasyenvState::empty();
        let token = state.encode();
        assert_eq!(EasyenvState::decode(&token), Some(state));
    }

    #[test]
    fn round_trips_mixed_managed_vars() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "FOO".to_string(),
            ManagedVar {
                prior: PriorValue::Set("orig".to_string()),
            },
        );
        managed.insert(
            "BAR".to_string(),
            ManagedVar {
                prior: PriorValue::Unset,
            },
        );
        let state = EasyenvState {
            managed,
            signature: 0xDEAD_BEEF_u64,
        };
        let token = state.encode();
        assert_eq!(EasyenvState::decode(&token), Some(state));
    }

    #[test]
    fn round_trips_values_with_newlines_equals_and_unicode() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "TRICKY".to_string(),
            ManagedVar {
                prior: PriorValue::Set("line1\nline2=value\u{1F600}".to_string()),
            },
        );
        let state = EasyenvState {
            managed,
            signature: 1,
        };
        let token = state.encode();
        assert_eq!(EasyenvState::decode(&token), Some(state));
    }

    #[test]
    fn round_trips_empty_string_value() {
        let mut managed = BTreeMap::new();
        managed.insert(
            "EMPTY".to_string(),
            ManagedVar {
                prior: PriorValue::Set(String::new()),
            },
        );
        let state = EasyenvState {
            managed,
            signature: 2,
        };
        let token = state.encode();
        assert_eq!(EasyenvState::decode(&token), Some(state));
    }

    #[test]
    fn decode_rejects_garbage() {
        assert_eq!(EasyenvState::decode("not valid base64!!"), None);
        assert_eq!(EasyenvState::decode(""), None);
    }

    #[test]
    fn decode_rejects_wrong_version() {
        let bytes = vec![99u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let token = STANDARD.encode(bytes);
        assert_eq!(EasyenvState::decode(&token), None);
    }
}
