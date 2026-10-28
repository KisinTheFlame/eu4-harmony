//! Bytes in an answer, such as a screenshot's PNG. JSON has no bytes, so a
//! [`Binary`] is an object of its own shape, which a client can tell from
//! any other value without knowing the capability:
//!
//! ```json
//! {"media_type": "image/png", "base64": "iVBORw0KGgo..."}
//! ```
//!
//! `base64` is standard base64 with padding (RFC 4648, section 4). The
//! command line writes such bytes to the file `--out` names.
use serde::{de, ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;

/// Bytes and what they are.
#[derive(Clone, PartialEq, Eq)]
pub struct Binary {
    /// Their media type, such as `image/png`.
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl Binary {
    pub fn new(media_type: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self { media_type: media_type.into(), bytes }
    }

    /// The bytes `value` holds, when it is a binary's JSON form: an object
    /// with a `media_type` and valid `base64`, and nothing else.
    pub fn from_json(value: &Value) -> Option<Self> {
        let fields = value.as_object()?;
        if fields.len() != 2 {
            return None;
        }
        let media_type = fields.get("media_type")?.as_str()?;
        let bytes = decode(fields.get("base64")?.as_str()?)?;
        Some(Self::new(media_type, bytes))
    }
}

impl fmt::Debug for Binary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Binary({}, {} bytes)", self.media_type, self.bytes.len())
    }
}

impl Serialize for Binary {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Binary", 2)?;
        s.serialize_field("media_type", &self.media_type)?;
        s.serialize_field("base64", &encode(&self.bytes))?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Binary {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Form {
            media_type: String,
            base64: String,
        }
        let form = Form::deserialize(deserializer)?;
        let bytes = decode(&form.base64).ok_or_else(|| de::Error::custom("base64 is not valid base64"))?;
        Ok(Self::new(form.media_type, bytes))
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` in base64, padded.
pub fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = chunk.iter().enumerate().fold(0u32, |group, (i, &b)| group | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                text.push(ALPHABET[(group >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// The bytes padded base64 `text` stands for; None when it is not that.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    for (index, quad) in text.chunks(4).enumerate() {
        let last = index == text.len() / 4 - 1;
        let padding = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if padding > 2 || (padding > 0 && !last) {
            return None;
        }
        let mut group = 0u32;
        for &c in &quad[..4 - padding] {
            let value = ALPHABET.iter().position(|&a| a == c)? as u32;
            group = group << 6 | value;
        }
        group <<= 6 * padding as u32;
        let decoded = [(group >> 16) as u8, (group >> 8) as u8, group as u8];
        let count = 3 - padding;
        // Bits beyond the last byte must be zero, so each text has one form.
        if decoded[count..].iter().any(|&b| b != 0) {
            return None;
        }
        bytes.extend_from_slice(&decoded[..count]);
    }
    Some(bytes)
}
