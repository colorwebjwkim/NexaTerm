//! NexaTerm patch: SFTP handles are opaque bytes, never lossy UTF-8.
//! Keep the upstream String API with a reversible internal surrogate for binary handles.
use serde::{Deserialize, Deserializer, Serializer};
const PREFIX: &str = "\u{e000}nxt-handle:";
pub fn serialize<S: Serializer>(value: &str, serializer: S) -> Result<S::Ok, S::Error> {
    if let Some(hex) = value.strip_prefix(PREFIX) {
        if !hex.len().is_multiple_of(2) { return Err(serde::ser::Error::custom("invalid opaque handle")); }
        let mut bytes = Vec::with_capacity(hex.len()/2);
        for pair in hex.as_bytes().chunks_exact(2) {
            let pair = std::str::from_utf8(pair).map_err(serde::ser::Error::custom)?;
            bytes.push(u8::from_str_radix(pair,16).map_err(serde::ser::Error::custom)?);
        }
        serializer.serialize_bytes(&bytes)
    } else { serializer.serialize_bytes(value.as_bytes()) }
}
pub fn deserialize<'de,D: Deserializer<'de>>(deserializer: D) -> Result<String,D::Error> {
    let bytes = serde_bytes::ByteBuf::deserialize(deserializer)?.into_vec();
    if let Ok(text) = std::str::from_utf8(&bytes) { if !text.starts_with(PREFIX) { return Ok(text.to_owned()); } }
    let mut encoded=String::with_capacity(PREFIX.len()+bytes.len()*2);encoded.push_str(PREFIX);
    for byte in bytes { use std::fmt::Write; write!(&mut encoded,"{byte:02x}").map_err(serde::de::Error::custom)?; }
    Ok(encoded)
}
