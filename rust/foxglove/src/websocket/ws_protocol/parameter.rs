//! Parameter types.

use std::collections::BTreeMap;

use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

/// Error encountered while trying to decide a base64-encoded byte array parameter value.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// Parameter is not a byte-array.
    #[error("Parameter is not a byte array")]
    WrongType,
    /// Invalid base64.
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
}

/// A parameter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    /// A byte array, encoded as a base64-encoded string.
    ByteArray,
    /// A decimal or integer value that can be represented as a `float64`.
    Float64,
    /// An array of decimal or integer values that can be represented as `float64`s.
    Float64Array,
}

/// A parameter value.
#[serde_as]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParameterValue {
    /// A decimal or integer value.
    Number(f64),
    /// A boolean value.
    Bool(bool),
    /// A string value.
    ///
    /// For parameters of type [`ParameterType::ByteArray`], this is a base64 encoding of the byte
    /// array.
    String(String),
    /// An array of parameter values.
    Array(Vec<ParameterValue>),
    /// An associative map of parameter values.
    Dict(BTreeMap<String, ParameterValue>),
}

/// Informs the client about a parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// The name of the parameter.
    pub name: String,
    /// The parameter type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ParameterType>,
    /// The parameter value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ParameterValue>,
}

impl Parameter {
    /// Creates a new parameter with no value or type.
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            r#type: None,
            value: None,
        }
    }

    /// Creates a new parameter with a float64 value.
    pub fn float64(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            r#type: Some(ParameterType::Float64),
            value: Some(ParameterValue::Number(value)),
        }
    }

    /// Creates a new parameter with a float64 array value.
    pub fn float64_array(name: impl Into<String>, values: impl IntoIterator<Item = f64>) -> Self {
        Self {
            name: name.into(),
            r#type: Some(ParameterType::Float64Array),
            value: Some(ParameterValue::Array(
                values.into_iter().map(ParameterValue::Number).collect(),
            )),
        }
    }

    /// Creates a new parameter with a string value.
    pub fn string(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            r#type: None,
            value: Some(ParameterValue::String(value.into())),
        }
    }

    /// Creates a new parameter with a byte array value.
    pub fn byte_array(name: impl Into<String>, data: &[u8]) -> Self {
        let value = BASE64_STANDARD.encode(data);
        Self {
            name: name.into(),
            r#type: Some(ParameterType::ByteArray),
            value: Some(ParameterValue::String(value)),
        }
    }

    /// Creates a new parameter with a boolean value.
    pub fn bool(name: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            r#type: None,
            value: Some(ParameterValue::Bool(value)),
        }
    }

    /// Creates a new parameter with a dictionary value.
    pub fn dict(name: impl Into<String>, value: BTreeMap<String, ParameterValue>) -> Self {
        Self {
            name: name.into(),
            r#type: None,
            value: Some(ParameterValue::Dict(value)),
        }
    }

    /// Decodes a byte array parameter.
    ///
    /// Returns None if the parameter is unset/empty. Returns an error if the parameter value is
    /// not a byte array, or if it is not a valid base64 encoding.
    pub fn decode_byte_array(&self) -> Result<Option<Vec<u8>>, DecodeError> {
        match (self.r#type, self.value.as_ref()) {
            (Some(ParameterType::ByteArray), Some(ParameterValue::String(s))) => {
                Some(BASE64_STANDARD.decode(s).map_err(DecodeError::Base64)).transpose()
            }
            (_, None) => Ok(None),
            _ => Err(DecodeError::WrongType),
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    fn test_empty() {
        insta::assert_json_snapshot!(Parameter::empty("test"));
    }

    #[test]
    fn test_float() {
        insta::assert_json_snapshot!(Parameter::float64("f64", 1.23));
    }

    #[test]
    fn test_float_array() {
        insta::assert_json_snapshot!(Parameter::float64_array("f64[]", [1.23, 4.56]));
    }

    #[test]
    fn test_string() {
        insta::assert_json_snapshot!(Parameter::string("string", "howdy"));
    }

    #[test]
    fn test_byte_array() {
        insta::assert_json_snapshot!(Parameter::byte_array("byte[]", &[0x10, 0x20, 0x30]));
    }

    #[test]
    fn test_decode_byte_array() {
        let param = Parameter::byte_array("bytes", b"123");
        let decoded = param.decode_byte_array().unwrap().unwrap();
        assert_eq!(decoded, b"123".to_vec());

        // invalid base64 value
        let param = Parameter {
            name: "invalid".into(),
            r#type: Some(ParameterType::ByteArray),
            value: Some(ParameterValue::String("!!".into())),
        };
        let result = param.decode_byte_array();
        assert_matches!(result, Err(DecodeError::Base64(_)));

        // it's a string, not a byte array
        let param = Parameter::string("string", "eHl6enk=");
        let result = param.decode_byte_array();
        assert_matches!(result, Err(DecodeError::WrongType));

        // unset
        let param = Parameter {
            name: "unset".into(),
            r#type: Some(ParameterType::ByteArray),
            value: None,
        };
        let result = param.decode_byte_array();
        assert_matches!(result, Ok(None));

        // unset of a different type
        let param = Parameter {
            name: "unset".into(),
            r#type: None,
            value: None,
        };
        let result = param.decode_byte_array();
        assert_matches!(result, Ok(None));
    }

    #[test]
    fn test_bool() {
        insta::assert_json_snapshot!(Parameter::bool("bool", true));
    }

    #[test]
    fn test_dict() {
        insta::assert_json_snapshot!(Parameter::dict(
            "outer",
            maplit::btreemap! {
                "bool".into() => ParameterValue::Bool(false),
                "number".into() => ParameterValue::Number(1.23),
                "nested".into() => ParameterValue::Dict(
                    maplit::btreemap! {
                        "inner".into() => ParameterValue::Number(1.0),
                    }
                )
            }
        ));
    }
}
