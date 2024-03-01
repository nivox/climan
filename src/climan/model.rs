use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, JsonSchema, strum::Display, Clone)]
pub enum Method {
    #[serde(alias = "get")]
    GET,
    #[serde(alias = "post")]
    POST,
    #[serde(alias = "put")]
    PUT,
    #[serde(alias = "delete")]
    DELETE,
    #[serde(alias = "patch")]
    PATCH,
    #[serde(alias = "head")]
    HEAD,
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(untagged)]
pub enum ParamValue {
    StringParam(String),
    NumberParam(f32),
    BoolParam(bool),
    ListParam(Vec<serde_json::Value>),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(untagged)]
pub enum Body {
    File { file: String },
    Content { content: String, trim: Option<bool> },
}

impl Body {
    pub fn content(&self) -> Vec<u8> {
        match self {
            Body::File { file } => std::fs::read(file).unwrap(),
            Body::Content { content, trim } => {
                let value = if trim.unwrap_or(false) {
                    content.trim()
                } else {
                    content
                };
                value.as_bytes().to_vec()
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, JsonSchema, Clone)]
#[serde(tag = "type")]
pub enum Authentication {
    #[serde(rename = "basic")]
    Basic {
        username: String,
        password: Option<String>,
    },

    #[serde(rename = "bearer")]
    Bearer { token: String },
}
