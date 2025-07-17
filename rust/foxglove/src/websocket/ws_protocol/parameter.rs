//! Parameter types.

use std::collections::BTreeMap;

use base64::prelude::*;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
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
    /// A floating-point value that can be represented as a `float64`.
    Float64,
    /// An array of floating-point values that can be represented as `float64`s.
    Float64Array,
}

/// A parameter value.
#[serde_as]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParameterValue {
    /// An integer value.
    Integer(i64),
    /// A floating-point value.
    Float64(f64),
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
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parameter {
    /// The name of the parameter.
    pub name: String,
    /// The parameter type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ParameterType>,
    /// The parameter value. If None, the parameter is treated as unset/deleted, and will not
    /// be published to clients.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ParameterValue>,
}

impl<'de> Deserialize<'de> for Parameter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ParameterVisitor;

        impl<'de> Visitor<'de> for ParameterVisitor {
            type Value = Parameter;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a parameter object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut name: Option<String> = None;
                let mut r#type: Option<ParameterType> = None;
                let mut value: Option<serde_json::Value> = None;

                // First pass: collect all fields as raw values
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            name = Some(map.next_value()?);
                        }
                        "type" => {
                            r#type = Some(map.next_value()?);
                        }
                        "value" => {
                            value = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(de::Error::unknown_field(&key, &["name", "type", "value"]));
                        }
                    }
                }

                let name = name.ok_or_else(|| de::Error::missing_field("name"))?;

                // Convert value based on type
                let parameter_value = match (r#type.as_ref(), value) {
                    (Some(ParameterType::Float64), Some(val)) => {
                        Some(convert_to_float64_value(val).map_err(de::Error::custom)?)
                    }
                    (Some(ParameterType::Float64Array), Some(val)) => {
                        Some(convert_to_float64_array_value(val).map_err(de::Error::custom)?)
                    }
                    (Some(ParameterType::ByteArray), Some(val)) => {
                        Some(convert_to_byte_array_value(val).map_err(de::Error::custom)?)
                    }
                    (_, Some(val)) => {
                        Some(convert_value_with_homogenization(val).map_err(de::Error::custom)?)
                    }
                    (_, None) => None,
                };

                Ok(Parameter {
                    name,
                    r#type,
                    value: parameter_value,
                })
            }
        }

        deserializer.deserialize_map(ParameterVisitor)
    }
}

fn convert_to_float64_value(value: serde_json::Value) -> Result<ParameterValue, String> {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(ParameterValue::Float64(f))
            } else {
                Err("Invalid number for float64".to_string())
            }
        }
        _ => {
            // For non-numeric values marked as float64, raise an error
            Err("Non-numeric value had type set to float64".to_string())
        }
    }
}

fn convert_to_float64_array_value(value: serde_json::Value) -> Result<ParameterValue, String> {
    match value {
        serde_json::Value::Array(arr) => {
            let mut float_values = Vec::new();
            for item in arr {
                match item {
                    serde_json::Value::Number(n) => {
                        if let Some(f) = n.as_f64() {
                            float_values.push(ParameterValue::Float64(f));
                        } else {
                            return Err("Invalid number in float64 array".to_string());
                        }
                    }
                    _ => {
                        return Err("Non-numeric value in float64 array".to_string());
                    }
                }
            }
            Ok(ParameterValue::Array(float_values))
        }
        _ => {
            // For non-array values marked as float64_array, raise an error
            Err("Value with type set to float64_array was not an array of numbers".to_string())
        }
    }
}

fn convert_to_byte_array_value(value: serde_json::Value) -> Result<ParameterValue, String> {
    match value {
        serde_json::Value::String(s) => {
            // Check if the string is a valid base64 encoding
            if let Err(e) = BASE64_STANDARD.decode(&s) {
                return Err(e.to_string());
            }
            Ok(ParameterValue::String(s))
        }
        _ => {
            // For non-string values marked as byte_array, raise an error
            Err("Value with type set to byte_array was not a string".to_string())
        }
    }
}

fn convert_value_with_homogenization(value: serde_json::Value) -> Result<ParameterValue, String> {
    match value {
        serde_json::Value::Array(arr) => {
            // Check if array contains mixed numeric types
            let mut has_int = false;
            let mut has_float = false;
            let mut has_other = false;

            for item in &arr {
                if item.is_i64() {
                    has_int = true;
                } else if item.is_f64() {
                    has_float = true;
                } else {
                    has_other = true;
                }
            }

            if (has_float || has_int) && has_other {
                // If the array contains a mix of numeric and non-numeric-values, return an error
                Err("Array contains a mix of numeric and non-numeric-values".to_string())
            } else if has_int && has_float {
                // Homogenize to float64 array
                let mut float_values = Vec::new();
                for item in arr {
                    match item {
                        serde_json::Value::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                float_values.push(ParameterValue::Float64(f));
                            } else {
                                return Err("Invalid number in mixed array".to_string());
                            }
                        }
                        _ => {
                            unreachable!()
                        }
                    }
                }
                Ok(ParameterValue::Array(float_values))
            } else {
                // Array was already homogeneous, use normal deserialization
                serde_json::from_value(serde_json::Value::Array(arr)).map_err(|e| e.to_string())
            }
        }
        _ => {
            // For non-array values, use normal deserialization
            serde_json::from_value(value).map_err(|e| e.to_string())
        }
    }
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
            value: Some(ParameterValue::Float64(value)),
        }
    }

    /// Creates a new parameter with an integer value.
    pub fn integer(name: impl Into<String>, value: i64) -> Self {
        Self {
            name: name.into(),
            r#type: None,
            value: Some(ParameterValue::Integer(value)),
        }
    }

    /// Creates a new parameter with an integer array value.
    pub fn integer_array(name: impl Into<String>, values: impl IntoIterator<Item = i64>) -> Self {
        Self {
            name: name.into(),
            r#type: None,
            value: Some(ParameterValue::Array(
                values.into_iter().map(ParameterValue::Integer).collect(),
            )),
        }
    }

    /// Creates a new parameter with a float64 array value.
    pub fn float64_array(name: impl Into<String>, values: impl IntoIterator<Item = f64>) -> Self {
        Self {
            name: name.into(),
            r#type: Some(ParameterType::Float64Array),
            value: Some(ParameterValue::Array(
                values.into_iter().map(ParameterValue::Float64).collect(),
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
    fn test_integer() {
        insta::assert_json_snapshot!(Parameter::integer("i64", 123));
    }

    #[test]
    fn test_integer_array() {
        insta::assert_json_snapshot!(Parameter::integer_array("i64[]", [123, 456]));
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
    fn test_deserialize_integer() {
        let json = r#"{"name": "test", "value": 123}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is an integer
        assert_matches!(param.value, Some(ParameterValue::Integer(_)));
        assert_eq!(param.value.unwrap(), ParameterValue::Integer(123));
        assert_eq!(param.r#type, None);
    }

    #[test]
    fn test_deserialize_integer_array() {
        let json = r#"{"name": "test", "value": [123, 456]}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is an integer array
        assert_matches!(param.value, Some(ParameterValue::Array(_)));
        assert_eq!(
            param.value.unwrap(),
            ParameterValue::Array(vec![
                ParameterValue::Integer(123),
                ParameterValue::Integer(456)
            ])
        );
    }

    #[test]
    fn test_deserialize_integer_marked_as_float64() {
        let json = r#"{"name": "test", "value": 123, "type": "float64"}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is a float64
        assert_matches!(param.value, Some(ParameterValue::Float64(_)));
        assert_eq!(param.value.unwrap(), ParameterValue::Float64(123.0));
        assert_eq!(param.r#type, Some(ParameterType::Float64));
    }

    #[test]
    fn test_deserialize_integer_array_marked_as_float64() {
        let json = r#"{"name": "test", "value": [123, 456], "type": "float64_array"}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is a float64 array
        assert_matches!(param.value, Some(ParameterValue::Array(_)));
        assert_eq!(param.r#type, Some(ParameterType::Float64Array));
        assert_eq!(
            param.value.unwrap(),
            ParameterValue::Array(vec![
                ParameterValue::Float64(123.0),
                ParameterValue::Float64(456.0),
            ])
        );
    }

    #[test]
    fn test_deserialize_float64() {
        let json = r#"{"name": "test", "value": 1.23}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is a float64
        assert_matches!(param.value, Some(ParameterValue::Float64(_)));
        assert_eq!(param.value.unwrap(), ParameterValue::Float64(1.23));
    }

    #[test]
    fn test_deserialize_float64_array() {
        let json = r#"{"name": "test", "value": [1.23, 4.56]}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is a float64 array
        assert_matches!(param.value, Some(ParameterValue::Array(_)));
        assert_eq!(
            param.value.unwrap(),
            ParameterValue::Array(vec![
                ParameterValue::Float64(1.23),
                ParameterValue::Float64(4.56),
            ])
        );
    }

    #[test]
    fn test_deserialize_numeric_parameter_with_zero_fractional_part() {
        let json = r#"{"name": "test", "value": 1.0}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is a float64
        assert_matches!(param.value, Some(ParameterValue::Float64(_)));
        assert_eq!(param.value.unwrap(), ParameterValue::Float64(1.0));
    }

    #[test]
    fn test_deserialize_numeric_array_with_zero_fractional_part() {
        let json = r#"{"name": "test", "value": [1.0, 2.0]}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // Check that the value is a float64 array
        assert_matches!(param.value, Some(ParameterValue::Array(_)));
        assert_eq!(
            param.value.unwrap(),
            ParameterValue::Array(vec![
                ParameterValue::Float64(1.0),
                ParameterValue::Float64(2.0)
            ])
        );
    }

    #[test]
    fn test_deserialize_numeric_array_with_heterogeneous_elements() {
        let json = r#"{"name": "test", "value": [1, 2.0]}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        // In the case of mixed types, the value is a float64 array
        assert_matches!(param.value, Some(ParameterValue::Array(_)));
        assert_eq!(
            param.value.unwrap(),
            ParameterValue::Array(vec![
                ParameterValue::Float64(1.0),
                ParameterValue::Float64(2.0)
            ])
        );
    }

    #[test]
    fn test_deserialize_boolean() {
        let json = r#"{"name": "test", "value": true}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        assert_matches!(param.value, Some(ParameterValue::Bool(_)));
        assert_eq!(param.value.unwrap(), ParameterValue::Bool(true));
    }

    #[test]
    fn test_deserialize_boolean_array() {
        let json = r#"{"name": "test", "value": [true, false]}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        assert_matches!(param.value, Some(ParameterValue::Array(_)));
        assert_eq!(
            param.value.unwrap(),
            ParameterValue::Array(vec![
                ParameterValue::Bool(true),
                ParameterValue::Bool(false)
            ])
        );
    }

    #[test]
    fn test_deserialize_byte_array() {
        let json = r#"{"name": "test", "value": "Rm94Z2xvdmUgcnVsZXMh", "type": "byte_array"}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        assert_matches!(param.value, Some(ParameterValue::String(_)));
        assert_eq!(
            param.decode_byte_array().unwrap().unwrap(),
            b"Foxglove rules!".to_vec()
        );
    }

    #[test]
    fn test_deserialize_undefined_parameter() {
        let json = r#"{"name": "test"}"#;
        let param = serde_json::from_str::<Parameter>(json).unwrap();

        assert_eq!(param.name, "test");
        assert_matches!(param.value, None);
        assert_matches!(param.r#type, None);
    }

    #[test]
    fn test_deserialize_array_with_mixed_types() {
        let json = r#"{"name": "test", "value": [1, 2.0, "three"]}"#;
        let param_result = serde_json::from_str::<Parameter>(json);

        // Check that an error is returned
        assert_matches!(param_result, Err(_));
    }

    #[test]
    fn test_deserialize_invalid_float64_array() {
        let json = r#"{"name": "test", "value": [true, false, true], type: "float64_array"}"#;
        let param_result = serde_json::from_str::<Parameter>(json);

        // Check that an error is returned
        assert_matches!(param_result, Err(_));
    }

    #[test]
    fn test_deserialize_invalid_float64_value() {
        let json = r#"{"name": "test", "value": "three point one four one five", type: "float64"}"#;
        let param_result = serde_json::from_str::<Parameter>(json);

        // Check that an error is returned
        assert_matches!(param_result, Err(_));
    }

    #[test]
    fn test_deserialize_invalid_byte_array() {
        let json = r#"{"name": "test", "value": "!!!!", "type": "byte_array"}"#;
        let param_result = serde_json::from_str::<Parameter>(json);

        // Check that an error is returned
        assert_matches!(param_result, Err(_));
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
                "nested".into() => ParameterValue::Dict(
                    maplit::btreemap! {
                        "inner".into() => ParameterValue::Float64(1.0),
                    }
                ),
                "float64".into() => ParameterValue::Float64(1.23),
            }
        ));
    }
}
