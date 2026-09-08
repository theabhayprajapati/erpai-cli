use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Internal,
    Validation,
    Auth,
    Forbidden,
    Network,
    Api,
    NotFound,
}

impl ErrorCode {
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorCode::Internal => 1,
            ErrorCode::Validation => 2,
            ErrorCode::Auth => 3,
            ErrorCode::Forbidden => 4,
            ErrorCode::Network => 5,
            ErrorCode::Api => 6,
            ErrorCode::NotFound => 7,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Internal => "internal_error",
            ErrorCode::Validation => "validation_error",
            ErrorCode::Auth => "auth_error",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::Network => "network_error",
            ErrorCode::Api => "api_error",
            ErrorCode::NotFound => "not_found",
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<String>,
    pub request_id: Option<String>,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl CliError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            request_id: None,
        }
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, m)
    }
    pub fn validation(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, m)
    }
    pub fn auth(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Auth, m)
    }
    pub fn forbidden(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, m)
    }
    pub fn network(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Network, m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, m)
    }
    pub fn api(status: u16, m: impl Into<String>) -> Self {
        Self::new(ErrorCode::Api, format!("HTTP {status}: {}", m.into()))
    }
    pub fn with_hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }
    pub fn with_request_id(mut self, id: Option<String>) -> Self {
        self.request_id = id;
        self
    }

    pub fn to_json(&self) -> serde_json::Value {
        let mut e = serde_json::json!({ "code": self.code.as_str(), "message": self.message });
        if let Some(h) = &self.hint {
            e["hint"] = serde_json::Value::String(h.clone());
        }
        if let Some(r) = &self.request_id {
            e["requestId"] = serde_json::Value::String(r.clone());
        }
        serde_json::json!({ "error": e })
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::validation(format!("invalid JSON: {e}"))
    }
}
impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::internal(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CliError>;
