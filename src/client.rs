use crate::config::Profile;
use crate::error::{CliError, Result};
use reqwest::{Method, StatusCode};
use serde_json::Value;
use std::time::Duration;

pub struct ApiClient {
    base: String,
    key: String,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(profile: &Profile) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(format!("erpai-cli/{}", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| CliError::internal(e.to_string()))?;
        Ok(Self {
            base: profile.base_url.trim_end_matches('/').to_string(),
            key: profile.api_key.clone(),
            http,
        })
    }

    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        self.send(Method::GET, path, query, None, &[]).await
    }
    pub async fn post(&self, path: &str, query: &[(&str, &str)], body: &Value) -> Result<Value> {
        self.send(Method::POST, path, query, Some(body), &[]).await
    }
    pub async fn put(&self, path: &str, query: &[(&str, &str)], body: &Value) -> Result<Value> {
        self.send(Method::PUT, path, query, Some(body), &[]).await
    }
    pub async fn patch(&self, path: &str, query: &[(&str, &str)], body: &Value) -> Result<Value> {
        self.send(Method::PATCH, path, query, Some(body), &[]).await
    }
    pub async fn delete(
        &self,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value> {
        self.send(Method::DELETE, path, query, body, &[]).await
    }

    /// Any method with caller-supplied extra headers (the `erpai api --header` path). The
    /// authorization and accept headers are always the client's own.
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
        headers: &[(String, String)],
    ) -> Result<Value> {
        self.send(method, path, query, body, headers).await
    }

    /// Multipart upload (the one non-JSON request shape). Same auth, error mapping and public-path rule.
    pub async fn post_multipart(
        &self,
        path: &str,
        query: &[(&str, &str)],
        form: reqwest::multipart::Form,
    ) -> Result<Value> {
        if !(path.starts_with("/v1/") || path.starts_with("/open/v1/")) {
            return Err(CliError::internal(format!(
                "refusing non-public path {path}"
            )));
        }
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .post(&url)
            .query(query)
            .header("authorization", format!("Bearer {}", self.key))
            .header("accept", "application/json")
            .multipart(form)
            .send()
            .await
            .map_err(|e| CliError::network(format!("POST {url}: {e}")))?;
        let status = resp.status();
        let request_id = resp
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let text = resp.text().await.unwrap_or_default();
        let json: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text.clone()))
        };
        if status.is_success() {
            Ok(json)
        } else {
            Err(map_error(status, &json).with_request_id(request_id))
        }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
        headers: &[(String, String)],
    ) -> Result<Value> {
        if !(path.starts_with("/v1/") || path.starts_with("/open/v1/")) {
            debug_assert!(false, "non-public path {path}");
            return Err(CliError::internal(format!(
                "refusing non-public path {path}"
            )));
        }
        let url = format!("{}{}", self.base, path);
        let mut attempt = 0u32;
        loop {
            let mut req = self
                .http
                .request(method.clone(), &url)
                .query(query)
                .header("authorization", format!("Bearer {}", self.key))
                .header("accept", "application/json");
            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    if attempt < 3 && (e.is_connect() || e.is_timeout()) {
                        attempt += 1;
                        backoff(attempt).await;
                        continue;
                    }
                    return Err(CliError::network(format!("{method} {url}: {e}")));
                }
            };
            let status = resp.status();
            let request_id = resp
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) && attempt < 3
            {
                attempt += 1;
                backoff(attempt).await;
                continue;
            }
            let text = resp.text().await.unwrap_or_default();
            let json: Value = if text.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&text).unwrap_or(Value::String(text.clone()))
            };
            if status.is_success() {
                return Ok(json);
            }
            return Err(map_error(status, &json).with_request_id(request_id));
        }
    }
}

async fn backoff(attempt: u32) {
    tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt - 1))).await
}

fn server_message(body: &Value) -> String {
    for k in ["message", "error", "msg"] {
        if let Some(s) = body.get(k).and_then(Value::as_str) {
            // validation errors carry the field-level reasons in details[]
            let details: Vec<String> = body
                .get("details")
                .and_then(Value::as_array)
                .map(|d| {
                    d.iter()
                        .filter_map(|e| {
                            let m = e.get("message").and_then(Value::as_str)?;
                            Some(match e.get("field").and_then(Value::as_str) {
                                Some(f) => format!("{f}: {m}"),
                                None => m.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            return if details.is_empty() {
                s.to_string()
            } else {
                format!("{s} — {}", details.join("; "))
            };
        }
    }
    match body {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string().chars().take(300).collect(),
    }
}

pub fn map_error(status: StatusCode, body: &Value) -> CliError {
    let msg = server_message(body);
    match status {
        StatusCode::UNAUTHORIZED => CliError::auth(if msg.is_empty() {
            "invalid or expired credential".into()
        } else {
            msg
        })
        .with_hint("run `erpai login`"),
        StatusCode::FORBIDDEN => {
            let allowed: Vec<String> = body
                .get("allowedApps")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let e = CliError::forbidden(if msg.is_empty() {
                "forbidden".into()
            } else {
                msg
            });
            if allowed.is_empty() {
                e
            } else {
                e.with_hint(format!("this key may access only: {}", allowed.join(", ")))
            }
        }
        StatusCode::NOT_FOUND => CliError::not_found(if msg.is_empty() {
            "not found".into()
        } else {
            msg
        }),
        s => CliError::api(
            s.as_u16(),
            if msg.is_empty() {
                s.canonical_reason().unwrap_or("error").to_string()
            } else {
                msg
            },
        ),
    }
}
