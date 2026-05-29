use super::*;
use splendor_types::{Action, QuotaUsage, SideEffectClass};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;
use time::OffsetDateTime;
use url::Url;

struct TestServer {
    url: String,
    handle: JoinHandle<()>,
}

impl TestServer {
    fn start(body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                Self::drain_request(&mut stream);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            url: format!("http://{addr}/"),
            handle,
        }
    }

    fn start_raw(response: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                Self::drain_request(&mut stream);
                let _ = stream.write_all(&response);
            }
        });
        Self {
            url: format!("http://{addr}/"),
            handle,
        }
    }

    fn drain_request(stream: &mut TcpStream) {
        let mut buffer = Vec::new();
        let mut temp = [0u8; 1024];
        let mut header_end = None;
        let mut expected_body = 0usize;
        loop {
            let read = match stream.read(&mut temp) {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => break,
            };
            buffer.extend_from_slice(&temp[..read]);
            if header_end.is_none() {
                if let Some(pos) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    header_end = Some(pos + 4);
                    let header_text = String::from_utf8_lossy(&buffer[..pos + 4]);
                    for line in header_text.lines() {
                        if let Some(value) = line.strip_prefix("Content-Length:") {
                            if let Ok(parsed) = value.trim().parse::<usize>() {
                                expected_body = parsed;
                            }
                        }
                    }
                }
            }
            if let Some(end) = header_end {
                if buffer.len() >= end + expected_body {
                    break;
                }
            }
        }
    }
}

fn build_action(name: &str, params: serde_json::Value) -> ActionRequest {
    ActionRequest {
        action_id: splendor_gateway::ActionId::new(),
        tenant_id: TenantId::new(),
        agent_id: splendor_types::AgentId::new(),
        run_id: splendor_types::RunId::new(),
        action: Action {
            name: name.to_string(),
            params,
            side_effect_class: SideEffectClass::Network,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        },
        adapter: None,
        quota_usage: QuotaUsage {
            actions: 1,
            http_requests: 1,
            ..QuotaUsage::default()
        },
        satisfied_preconditions: Vec::new(),
        requested_at: OffsetDateTime::now_utc(),
        approval_evidence: None,
    }
}

#[test]
fn http_get_succeeds_on_allowlisted_domain() {
    let server = TestServer::start("ok");
    let host = Url::parse(&server.url)
        .expect("url")
        .host_str()
        .expect("host")
        .to_string();
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec![host],
        ..HttpAdapterConfig::default()
    });

    let action = build_action(GET_ACTION, serde_json::json!({"url": server.url}));
    let result = adapter.execute(&action).expect("execute");
    assert_eq!(result.output["status"], 200);
    assert_eq!(result.output["body"], "ok");
    let _ = server.handle.join();
}

#[test]
fn http_get_with_headers_succeeds() {
    let server = TestServer::start("ok");
    let host = Url::parse(&server.url)
        .expect("url")
        .host_str()
        .expect("host")
        .to_string();
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec![host],
        ..HttpAdapterConfig::default()
    });

    let action = build_action(
        GET_ACTION,
        serde_json::json!({"url": server.url, "headers": {"X-Test": "yes"}}),
    );
    let result = adapter.execute(&action).expect("execute");
    assert_eq!(result.output["status"], 200);
    let _ = server.handle.join();
}

#[test]
fn http_denies_disallowed_domain() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let action = build_action(GET_ACTION, serde_json::json!({"url": "http://example.com"}));
    let error = adapter.execute(&action).expect_err("error");
    assert!(error.to_string().contains("allowlisted"));
}

#[test]
fn http_enforces_response_limit() {
    let server = TestServer::start("response-body");
    let host = Url::parse(&server.url)
        .expect("url")
        .host_str()
        .expect("host")
        .to_string();
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec![host],
        max_response_bytes: 4,
        ..HttpAdapterConfig::default()
    });

    let action = build_action(GET_ACTION, serde_json::json!({"url": server.url}));
    let error = adapter.execute(&action).expect_err("error");
    assert!(error.to_string().contains("response size"));
    let _ = server.handle.join();
}

#[test]
fn http_enforces_response_limit_without_content_length() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nresponse-body";
    let server = TestServer::start_raw(response.to_vec());
    let host = Url::parse(&server.url)
        .expect("url")
        .host_str()
        .expect("host")
        .to_string();
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec![host],
        max_response_bytes: 4,
        ..HttpAdapterConfig::default()
    });

    let action = build_action(GET_ACTION, serde_json::json!({"url": server.url}));
    let error = adapter.execute(&action).expect_err("response limit");
    assert!(error.to_string().contains("response size"));
    let _ = server.handle.join();
}

#[test]
fn http_allows_content_length_within_limit() {
    let server = TestServer::start("ok");
    let host = Url::parse(&server.url)
        .expect("url")
        .host_str()
        .expect("host")
        .to_string();
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec![host],
        max_response_bytes: 10,
        ..HttpAdapterConfig::default()
    });

    let action = build_action(GET_ACTION, serde_json::json!({"url": server.url}));
    let result = adapter.execute(&action).expect("execute");
    assert_eq!(result.output["status"], 200);
    assert_eq!(result.output["body"], "ok");
    let _ = server.handle.join();
}

#[test]
fn http_post_enforces_request_limit() {
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["127.0.0.1".to_string()],
        max_request_bytes: 2,
        ..HttpAdapterConfig::default()
    });

    let action = build_action(
        POST_ACTION,
        serde_json::json!({"url": "http://127.0.0.1:1/", "body": "abcd"}),
    );
    let error = adapter.execute(&action).expect_err("error");
    assert!(error.to_string().contains("request size"));
}

#[test]
fn tenant_domains_override_allowlist() {
    let tenant_id = TenantId::new();
    let mut tenant_domains = HashMap::new();
    tenant_domains.insert(tenant_id.clone(), vec!["tenant.local".to_string()]);
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["example.com".to_string()],
        tenant_domains,
        ..HttpAdapterConfig::default()
    });

    let allowlist = adapter.allowed_domains(&tenant_id);
    assert_eq!(allowlist, &["tenant.local".to_string()]);
    assert!(adapter.domain_allowed(&tenant_id, "tenant.local"));
    assert!(!adapter.domain_allowed(&tenant_id, "example.com"));

    let method = adapter.method_for_action(POST_ACTION).expect("method");
    assert_eq!(method, HttpMethod::Post);
    assert!(adapter.method_allowed(method));

    let (body, content_type) = adapter
        .extract_body(&serde_json::json!({"json": {"value": 1}}))
        .expect("body");
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert!(!body.is_empty());
}

#[test]
fn http_rejects_unsupported_action() {
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["example.com".to_string()],
        ..HttpAdapterConfig::default()
    });
    let action = build_action("http_put", serde_json::json!({"url": "http://example.com"}));
    let error = adapter.execute(&action).expect_err("unsupported action");
    assert!(error.to_string().contains("unsupported http action"));
}

#[test]
fn http_rejects_disallowed_method() {
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["example.com".to_string()],
        allowed_methods: vec![HttpMethod::Get],
        ..HttpAdapterConfig::default()
    });

    let action = build_action(
        POST_ACTION,
        serde_json::json!({"url": "http://example.com"}),
    );
    let error = adapter.execute(&action).expect_err("method denied");
    assert!(error.to_string().contains("method POST is not allowed"));
}

#[test]
fn http_post_sets_content_type_for_json() {
    let server = TestServer::start("ok");
    let host = Url::parse(&server.url)
        .expect("url")
        .host_str()
        .expect("host")
        .to_string();
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec![host],
        ..HttpAdapterConfig::default()
    });

    let action = build_action(
        POST_ACTION,
        serde_json::json!({"url": server.url, "json": {"value": 1}}),
    );
    let result = adapter.execute(&action).expect("execute");
    assert_eq!(result.output["status"], 200);
    assert_eq!(result.output["method"], "POST");
    let _ = server.handle.join();
}

#[test]
fn domain_allowlist_supports_wildcards() {
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["*.example.com".to_string(), ".example.org".to_string()],
        ..HttpAdapterConfig::default()
    });
    let tenant_id = TenantId::new();
    assert!(adapter.domain_allowed(&tenant_id, "api.example.com"));
    assert!(adapter.domain_allowed(&tenant_id, "svc.example.org"));
    assert!(!adapter.domain_allowed(&tenant_id, "example.net"));
}

#[test]
fn extract_url_requires_value() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let error = adapter
        .extract_url(&serde_json::json!({}))
        .expect_err("missing url");
    assert!(error.to_string().contains("missing url"));
}

#[test]
fn extract_headers_validates_types() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let error = adapter
        .extract_headers(&serde_json::json!({"headers": ["bad"]}))
        .expect_err("headers must be object");
    assert!(error.to_string().contains("headers must be an object"));

    let error = adapter
        .extract_headers(&serde_json::json!({"headers": {"X-Test": 1}}))
        .expect_err("header values must be strings");
    assert!(error.to_string().contains("header values"));
}

#[test]
fn extract_headers_accepts_valid_map() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let headers = adapter
        .extract_headers(&serde_json::json!({"headers": {"X-Test": "ok"}}))
        .expect("headers");
    assert_eq!(headers, vec![("X-Test".to_string(), "ok".to_string())]);
}

#[test]
fn extract_body_handles_json_and_bytes() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let (body, content_type) = adapter
        .extract_body(&serde_json::json!({"json": {"k": 1}}))
        .expect("json body");
    assert_eq!(content_type.as_deref(), Some("application/json"));
    assert!(!body.is_empty());

    let (body, content_type) = adapter
        .extract_body(&serde_json::json!({"bytes": [1, 2, 3]}))
        .expect("bytes body");
    assert_eq!(content_type, None);
    assert_eq!(body, vec![1, 2, 3]);

    let error = adapter
        .extract_body(&serde_json::json!({"bytes": [300]}))
        .expect_err("byte out of range");
    assert!(error.to_string().contains("byte value"));
}

#[test]
fn extract_body_accepts_string() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let (body, content_type) = adapter
        .extract_body(&serde_json::json!({"body": "hello"}))
        .expect("body");
    assert_eq!(content_type, None);
    assert_eq!(body, b"hello".to_vec());
}

#[test]
fn extract_body_defaults_to_empty() {
    let adapter = HttpAdapter::new(HttpAdapterConfig::default());
    let (body, content_type) = adapter.extract_body(&serde_json::json!({})).expect("body");
    assert!(body.is_empty());
    assert_eq!(content_type, None);
}

#[test]
fn http_returns_bytes_for_non_utf8_body() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Type: application/octet-stream\r\n\r\n\xFF\xFE";
    let server = TestServer::start_raw(response.to_vec());
    let host = Url::parse(&server.url)
        .expect("url")
        .host_str()
        .expect("host")
        .to_string();
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec![host],
        ..HttpAdapterConfig::default()
    });

    let action = build_action(GET_ACTION, serde_json::json!({"url": server.url}));
    let result = adapter.execute(&action).expect("execute");
    assert_eq!(result.output["status"], 200);
    assert!(result.output["body"].is_array());
    assert_eq!(result.output["body"], serde_json::json!([255, 254]));
    let _ = server.handle.join();
}

#[test]
fn execute_rejects_missing_host() {
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["127.0.0.1".to_string()],
        ..HttpAdapterConfig::default()
    });
    let action = build_action(GET_ACTION, serde_json::json!({"url": "file:///tmp"}));
    let error = adapter.execute(&action).expect_err("missing host");
    assert!(error.to_string().contains("url must include host"));
}

#[test]
fn execute_requires_params_object() {
    let adapter = HttpAdapter::new(HttpAdapterConfig {
        allowed_domains: vec!["example.com".to_string()],
        ..HttpAdapterConfig::default()
    });
    let action = build_action(GET_ACTION, serde_json::json!("oops"));
    let error = adapter.execute(&action).expect_err("params must be object");
    assert!(error.to_string().contains("params must be an object"));
}
