//! # HTTP Adapter
//!
//! Provides safe, allowlisted HTTP access behind the action gateway.

use splendor_gateway::{ActionAdapter, ActionRequest, AdapterError, AdapterResult};
use splendor_types::TenantId;
use std::collections::HashMap;
use std::io::Read;
use std::time::{Duration, Instant};
use url::Url;

const GET_ACTION: &str = "http_get";
const POST_ACTION: &str = "http_post";

/// HTTP methods supported by the adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    /// HTTP GET method.
    Get,
    /// HTTP POST method.
    Post,
}

impl HttpMethod {
    fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
        }
    }
}

/// Configuration for the HTTP adapter.
#[derive(Clone, Debug)]
pub struct HttpAdapterConfig {
    /// Allowlisted domains for all tenants.
    pub allowed_domains: Vec<String>,
    /// Per-tenant allowlist overrides.
    pub tenant_domains: HashMap<TenantId, Vec<String>>,
    /// HTTP methods permitted by this adapter.
    pub allowed_methods: Vec<HttpMethod>,
    /// Request timeout for connect/read/write.
    pub timeout: Duration,
    /// Maximum response size in bytes.
    pub max_response_bytes: usize,
    /// Maximum request body size in bytes.
    pub max_request_bytes: usize,
    /// User agent header for outgoing requests.
    pub user_agent: String,
}

impl Default for HttpAdapterConfig {
    fn default() -> Self {
        Self {
            allowed_domains: Vec::new(),
            tenant_domains: HashMap::new(),
            allowed_methods: vec![HttpMethod::Get, HttpMethod::Post],
            timeout: Duration::from_secs(5),
            max_response_bytes: 512 * 1024,
            max_request_bytes: 256 * 1024,
            user_agent: "splendor-http-adapter/0.1".to_string(),
        }
    }
}

/// HTTP adapter with allowlist and size limits.
#[derive(Clone, Debug)]
pub struct HttpAdapter {
    config: HttpAdapterConfig,
    agent: ureq::Agent,
}

impl HttpAdapter {
    /// Creates a new HTTP adapter with the provided configuration.
    pub fn new(config: HttpAdapterConfig) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(config.timeout)
            .timeout_read(config.timeout)
            .timeout_write(config.timeout)
            .user_agent(&config.user_agent)
            .build();
        Self { config, agent }
    }

    fn method_for_action(&self, action_name: &str) -> Result<HttpMethod, AdapterError> {
        match action_name {
            GET_ACTION => Ok(HttpMethod::Get),
            POST_ACTION => Ok(HttpMethod::Post),
            _ => Err(AdapterError::Failed(format!(
                "unsupported http action {action_name}"
            ))),
        }
    }

    fn allowed_domains(&self, tenant_id: &TenantId) -> &[String] {
        self.config
            .tenant_domains
            .get(tenant_id)
            .map(|value| value.as_slice())
            .unwrap_or(&self.config.allowed_domains)
    }

    fn domain_allowed(&self, tenant_id: &TenantId, host: &str) -> bool {
        let allowlist = self.allowed_domains(tenant_id);
        if allowlist.is_empty() {
            return false;
        }
        allowlist.iter().any(|entry| {
            if entry.starts_with("*.") {
                host.ends_with(&entry[1..])
            } else if entry.starts_with('.') {
                host.ends_with(entry)
            } else {
                host == entry
            }
        })
    }

    fn method_allowed(&self, method: HttpMethod) -> bool {
        self.config.allowed_methods.contains(&method)
    }

    fn extract_url(&self, params: &serde_json::Value) -> Result<Url, AdapterError> {
        let url = params
            .get("url")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AdapterError::Failed("missing url".to_string()))?;
        Url::parse(url).map_err(|error| AdapterError::Failed(error.to_string()))
    }

    fn extract_headers(
        &self,
        params: &serde_json::Value,
    ) -> Result<Vec<(String, String)>, AdapterError> {
        let Some(headers) = params.get("headers") else {
            return Ok(Vec::new());
        };
        let map = headers
            .as_object()
            .ok_or_else(|| AdapterError::Failed("headers must be an object".to_string()))?;
        let mut output = Vec::with_capacity(map.len());
        for (key, value) in map {
            let value = value
                .as_str()
                .ok_or_else(|| AdapterError::Failed("header values must be strings".to_string()))?;
            output.push((key.clone(), value.to_string()));
        }
        Ok(output)
    }

    fn extract_body(
        &self,
        params: &serde_json::Value,
    ) -> Result<(Vec<u8>, Option<String>), AdapterError> {
        if let Some(json) = params.get("json") {
            let body = serde_json::to_vec(json)
                .map_err(|error| AdapterError::Failed(error.to_string()))?;
            return Ok((body, Some("application/json".to_string())));
        }
        if let Some(body) = params.get("body") {
            let text = body
                .as_str()
                .ok_or_else(|| AdapterError::Failed("body must be a string".to_string()))?;
            return Ok((text.as_bytes().to_vec(), None));
        }
        if let Some(bytes) = params.get("bytes") {
            let array = bytes
                .as_array()
                .ok_or_else(|| AdapterError::Failed("bytes must be an array".to_string()))?;
            let mut output = Vec::with_capacity(array.len());
            for value in array {
                let byte = value
                    .as_u64()
                    .ok_or_else(|| AdapterError::Failed("bytes must be integers".to_string()))?;
                if byte > u8::MAX as u64 {
                    return Err(AdapterError::Failed("byte value out of range".to_string()));
                }
                output.push(byte as u8);
            }
            return Ok((output, None));
        }
        Ok((Vec::new(), None))
    }

    fn read_response(
        &self,
        response: ureq::Response,
        start: Instant,
        url: &Url,
        method: HttpMethod,
    ) -> Result<AdapterResult, AdapterError> {
        let status = response.status();
        let content_type = response
            .header("Content-Type")
            .map(|value| value.to_string());
        if let Some(content_length) = response.header("Content-Length") {
            if let Ok(length) = content_length.parse::<usize>() {
                if length > self.config.max_response_bytes {
                    return Err(AdapterError::Failed(format!(
                        "response size {length} exceeds limit {}",
                        self.config.max_response_bytes
                    )));
                }
            }
        }

        let reader = response.into_reader();
        let mut bytes = Vec::new();
        reader
            .take(self.config.max_response_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| AdapterError::Failed(error.to_string()))?;
        if bytes.len() > self.config.max_response_bytes {
            return Err(AdapterError::Failed(format!(
                "response size {} exceeds limit {}",
                bytes.len(),
                self.config.max_response_bytes
            )));
        }

        let body_value = match String::from_utf8(bytes.clone()) {
            Ok(text) => serde_json::Value::String(text),
            Err(_) => serde_json::json!(bytes),
        };
        let output = serde_json::json!({
            "url": url.as_str(),
            "method": method.as_str(),
            "status": status,
            "bytes": bytes.len(),
            "duration_ms": start.elapsed().as_millis(),
            "content_type": content_type,
            "body": body_value,
        });
        Ok(AdapterResult {
            output,
            satisfied_postconditions: Vec::new(),
        })
    }
}

impl ActionAdapter for HttpAdapter {
    fn execute(&self, action: &ActionRequest) -> Result<AdapterResult, AdapterError> {
        let _ = action
            .action
            .params
            .as_object()
            .ok_or_else(|| AdapterError::Failed("params must be an object".to_string()))?;
        let url = self.extract_url(&action.action.params)?;
        let host = url
            .host_str()
            .ok_or_else(|| AdapterError::Failed("url must include host".to_string()))?;
        if !self.domain_allowed(&action.tenant_id, host) {
            return Err(AdapterError::Failed(format!(
                "domain not allowlisted: {host}"
            )));
        }

        let method = self.method_for_action(&action.action.name)?;
        if !self.method_allowed(method) {
            return Err(AdapterError::Failed(format!(
                "method {} is not allowed",
                method.as_str()
            )));
        }

        let mut request = match method {
            HttpMethod::Get => self.agent.get(url.as_str()),
            HttpMethod::Post => self.agent.post(url.as_str()),
        };
        request = request.set("User-Agent", &self.config.user_agent);

        for (key, value) in self.extract_headers(&action.action.params)? {
            request = request.set(&key, &value);
        }

        let start = Instant::now();
        let response = match method {
            HttpMethod::Get => request.call(),
            HttpMethod::Post => {
                let (body, content_type) = self.extract_body(&action.action.params)?;
                if body.len() > self.config.max_request_bytes {
                    return Err(AdapterError::Failed(format!(
                        "request size {} exceeds limit {}",
                        body.len(),
                        self.config.max_request_bytes
                    )));
                }
                if let Some(content_type) = content_type {
                    request = request.set("Content-Type", &content_type);
                }
                request.send_bytes(&body)
            }
        }
        .map_err(|error| AdapterError::Failed(error.to_string()))?;

        let mut result = self.read_response(response, start, &url, method)?;
        result.satisfied_postconditions = action.action.postconditions.clone();
        Ok(result)
    }
}

#[cfg(test)]
#[path = "../tests/unit/http_tests.rs"]
mod tests;
