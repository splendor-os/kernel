use super::*;
use splendor_types::{Action, QuotaUsage, SideEffectClass};
use std::fs;
use tempfile::TempDir;
use time::OffsetDateTime;

fn build_action(tenant_id: TenantId, name: &str, params: serde_json::Value) -> ActionRequest {
    ActionRequest {
        action_id: splendor_gateway::ActionId::new(),
        tenant_id,
        agent_id: splendor_types::AgentId::new(),
        run_id: splendor_types::RunId::new(),
        action: Action {
            name: name.to_string(),
            params,
            side_effect_class: SideEffectClass::Filesystem,
            cost_estimate: None,
            required_permissions: Vec::new(),
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        },
        adapter: None,
        quota_usage: QuotaUsage::single_action(),
        satisfied_preconditions: Vec::new(),
        requested_at: OffsetDateTime::now_utc(),
    }
}

#[test]
fn write_and_read_file_round_trip() {
    let temp = TempDir::new().expect("temp dir");
    let config = FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    };
    let adapter = FilesystemAdapter::new(config);
    let tenant_id = TenantId::new();

    let write_action = build_action(
        tenant_id.clone(),
        WRITE_ACTION,
        serde_json::json!({"path": "notes.txt", "contents": "hello"}),
    );
    let result = adapter.execute(&write_action).expect("write");
    assert_eq!(result.output["bytes_written"], 5);

    let read_action = build_action(
        tenant_id,
        READ_ACTION,
        serde_json::json!({"path": "notes.txt"}),
    );
    let result = adapter.execute(&read_action).expect("read");
    assert_eq!(
        result.output["bytes"],
        serde_json::json!([104, 101, 108, 108, 111])
    );
}

#[test]
fn list_dir_truncates_entries() {
    let temp = TempDir::new().expect("temp dir");
    let config = FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        max_list_entries: 1,
        ..FilesystemAdapterConfig::default()
    };
    let adapter = FilesystemAdapter::new(config);
    let tenant_id = TenantId::new();

    let root = temp.path().join(tenant_id.to_string());
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("a.txt"), b"a").expect("write a");
    fs::write(root.join("b.txt"), b"b").expect("write b");

    let action = build_action(tenant_id, LIST_ACTION, serde_json::json!({"path": "."}));
    let result = adapter.execute(&action).expect("list");
    assert_eq!(result.output["entries"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["truncated"], true);
}

#[test]
fn path_traversal_is_denied() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let action = build_action(
        tenant_id,
        READ_ACTION,
        serde_json::json!({"path": "../secret"}),
    );
    let error = adapter.execute(&action).expect_err("error");
    assert!(error.to_string().contains("path traversal"));
}

#[test]
fn read_limit_is_enforced() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        max_read_bytes: 2,
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let root = temp.path().join(tenant_id.to_string());
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("big.txt"), b"abcd").expect("write big");

    let action = build_action(
        tenant_id,
        READ_ACTION,
        serde_json::json!({"path": "big.txt"}),
    );
    let error = adapter.execute(&action).expect_err("error");
    assert!(error.to_string().contains("exceeds limit"));
}

#[test]
fn stat_returns_metadata() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let root = temp.path().join(tenant_id.to_string());
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("info.txt"), b"abc").expect("write info");

    let action = build_action(
        tenant_id,
        STAT_ACTION,
        serde_json::json!({"path": "info.txt"}),
    );
    let result = adapter.execute(&action).expect("stat");
    assert_eq!(result.output["is_file"], true);
    assert_eq!(result.output["is_dir"], false);
    assert_eq!(result.output["size"], 3);
}

#[test]
fn list_dir_defaults_to_root() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let root = temp.path().join(tenant_id.to_string());
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("root.txt"), b"root").expect("write root");

    let action = build_action(tenant_id, LIST_ACTION, serde_json::json!({}));
    let result = adapter.execute(&action).expect("list");
    let entries = result.output["entries"].as_array().expect("entries");
    assert!(entries.iter().any(|entry| entry["name"] == "root.txt"));
}

#[test]
fn write_bytes_array_round_trip() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let write_action = build_action(
        tenant_id.clone(),
        WRITE_ACTION,
        serde_json::json!({"path": "raw.bin", "bytes": [0, 1, 255]}),
    );
    let result = adapter.execute(&write_action).expect("write bytes");
    assert_eq!(result.output["bytes_written"], 3);

    let read_action = build_action(
        tenant_id,
        READ_ACTION,
        serde_json::json!({"path": "raw.bin"}),
    );
    let result = adapter.execute(&read_action).expect("read bytes");
    assert_eq!(result.output["bytes"], serde_json::json!([0, 1, 255]));
}

#[test]
fn write_rejects_invalid_bytes() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let action = build_action(
        tenant_id,
        WRITE_ACTION,
        serde_json::json!({"path": "bad.bin", "bytes": [256]}),
    );
    let error = adapter.execute(&action).expect_err("bytes out of range");
    assert!(error.to_string().contains("byte value"));
}

#[test]
fn write_rejects_missing_contents() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let action = build_action(
        tenant_id,
        WRITE_ACTION,
        serde_json::json!({"path": "empty.txt"}),
    );
    let error = adapter.execute(&action).expect_err("missing contents");
    assert!(error.to_string().contains("contents"));
}

#[test]
fn read_rejects_absolute_path() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let action = build_action(tenant_id, READ_ACTION, serde_json::json!({"path": "/tmp"}));
    let error = adapter.execute(&action).expect_err("absolute path");
    assert!(error.to_string().contains("absolute"));
}

#[test]
fn unsupported_action_is_rejected() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let action = build_action(tenant_id, "unknown", serde_json::json!({"path": "file"}));
    let error = adapter.execute(&action).expect_err("unsupported action");
    assert!(error.to_string().contains("unsupported"));
}

#[test]
fn params_must_be_object() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let action = build_action(tenant_id, READ_ACTION, serde_json::json!("oops"));
    let error = adapter.execute(&action).expect_err("params must be object");
    assert!(error.to_string().contains("params must be an object"));
}

#[test]
fn write_limit_is_enforced() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        max_write_bytes: 2,
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let action = build_action(
        tenant_id,
        WRITE_ACTION,
        serde_json::json!({"path": "big.txt", "contents": "abcd"}),
    );
    let error = adapter.execute(&action).expect_err("write limit");
    assert!(error.to_string().contains("write size"));
}

#[test]
fn write_fails_when_parent_is_file() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();

    let root = temp.path().join(tenant_id.to_string());
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("parent"), b"file").expect("write parent");

    let action = build_action(
        tenant_id,
        WRITE_ACTION,
        serde_json::json!({"path": "parent/child.txt", "contents": "data"}),
    );
    let error = adapter.execute(&action).expect_err("parent is file");
    let message = error.to_string().to_lowercase();
    assert!(message.contains("directory") || message.contains("file exists"));
}

#[test]
fn write_creates_nested_directories() {
    let temp = TempDir::new().expect("temp dir");
    let adapter = FilesystemAdapter::new(FilesystemAdapterConfig {
        base_dir: temp.path().to_path_buf(),
        ..FilesystemAdapterConfig::default()
    });
    let tenant_id = TenantId::new();
    let tenant_root = temp.path().join(tenant_id.to_string());

    let action = build_action(
        tenant_id.clone(),
        WRITE_ACTION,
        serde_json::json!({"path": "nested/dir/file.txt", "contents": "ok"}),
    );
    let result = adapter.execute(&action).expect("write");
    assert_eq!(result.output["bytes_written"], 2);
    let contents = fs::read_to_string(tenant_root.join("nested/dir/file.txt")).expect("read file");
    assert_eq!(contents, "ok");
}
