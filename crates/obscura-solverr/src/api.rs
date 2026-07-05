use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct RequestBody {
    pub cmd: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default, rename = "maxTimeout")]
    pub max_timeout: Option<u64>,
    #[serde(default, rename = "postData")]
    pub post_data: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Serialize)]
pub struct ResponseBody {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution: Option<Solution>,
}

#[derive(Debug, Serialize)]
pub struct Solution {
    pub url: String,
    pub status: u16,
    pub response: String,
    #[serde(rename = "userAgent")]
    pub user_agent: String,
    pub cookies: Vec<FlareCookie>,
}

#[derive(Debug, Serialize)]
pub struct FlareCookie {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl ResponseBody {
    pub fn ok(message: impl Into<String>, session: Option<String>, solution: Option<Solution>) -> Self {
        Self {
            status: "ok",
            message: Some(message.into()),
            session,
            solution,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            message: Some(message.into()),
            session: None,
            solution: None,
        }
    }
}
