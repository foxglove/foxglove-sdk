use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IntBinRequest {
    pub a: u64,
    pub b: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IntBinResponse {
    pub result: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SetBoolRequest {
    pub data: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct SetBoolResponse {
    pub success: bool,
    pub message: String,
}
