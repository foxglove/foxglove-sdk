//! Schema types.

use std::borrow::Cow;

use base64::prelude::*;

/// An error that occurs while encoding schema data.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// The schema data is not valid UTF-8.
    #[error("Schema data is not valid UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    /// Missing schema or schema_encoding.
    #[error("Missing schema or schema_encoding")]
    MissingSchema,
}
impl From<std::string::FromUtf8Error> for EncodeError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        EncodeError::Utf8(err.utf8_error())
    }
}

/// An error that occurs while decoding schema data.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The schema data is not valid base64.
    #[error("Schema data is not valid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    /// Missing schema or schema_encoding.
    #[error("Missing schema or schema_encoding")]
    MissingSchema,
}

/// Returns true if a schema is required for this message encoding.
pub(crate) fn is_schema_required(message_encoding: &str) -> bool {
    message_encoding == "flatbuffer"
        || message_encoding == "protobuf"
        || message_encoding == "ros1"
        || message_encoding == "cdr"
}

/// Returns true if the given `schema_encoding` is one of the types that is known to require binary
/// schema data (i.e. `protobuf` and `flatbuffer`). These require base64-encoding/decoding to be
/// sent via JSON messages on the websocket connection.
pub(crate) fn is_known_binary_schema_encoding(schema_encoding: impl AsRef<str>) -> bool {
    let schema_encoding = schema_encoding.as_ref();
    schema_encoding == "protobuf" || schema_encoding == "flatbuffer"
}

/// Encodes schema data, based on the schema encoding.
///
/// For binary encodings, the schema data is base64-encoded. For other encodings, the schema must
/// be valid UTF-8, or this function will return an error.
pub(crate) fn encode_schema_data<'a>(
    schema_encoding: &str,
    data: Cow<'a, [u8]>,
) -> Result<Cow<'a, str>, EncodeError> {
    if is_known_binary_schema_encoding(schema_encoding) {
        Ok(Cow::Owned(BASE64_STANDARD.encode(data)))
    } else {
        Ok(match data {
            Cow::Owned(data) => Cow::Owned(String::from_utf8(data)?),
            Cow::Borrowed(data) => Cow::Borrowed(std::str::from_utf8(data)?),
        })
    }
}

/// Decodes schema data, based on the schema encoding.
///
/// For binary encodings, the schema data is base64-encoded. For other encodings, the schema must
/// be valid UTF-8, or this function will return an error.
pub(crate) fn decode_schema_data(
    schema_encoding: &str,
    data: &str,
) -> Result<Vec<u8>, DecodeError> {
    if is_known_binary_schema_encoding(schema_encoding) {
        Ok(BASE64_STANDARD.decode(data)?)
    } else {
        Ok(data.as_bytes().to_vec())
    }
}

/// A schema for a channel or service.
#[derive(Debug, Clone)]
pub struct Schema<'a> {
    /// Schema name.
    pub name: Cow<'a, str>,
    /// Schema encoding.
    pub encoding: Cow<'a, str>,
    /// Schema data.
    pub data: Cow<'a, [u8]>,
}

impl<'a> Schema<'a> {
    /// Creates a new schema.
    pub fn new(
        name: impl Into<Cow<'a, str>>,
        encoding: impl Into<Cow<'a, str>>,
        data: impl Into<Cow<'a, [u8]>>,
    ) -> Self {
        Self {
            name: name.into(),
            encoding: encoding.into(),
            data: data.into(),
        }
    }

    /// Returns an owned version of this message.
    pub fn into_owned(self) -> Schema<'static> {
        Schema {
            name: self.name.into_owned().into(),
            encoding: self.encoding.into_owned().into(),
            data: self.data.into_owned().into(),
        }
    }
}
