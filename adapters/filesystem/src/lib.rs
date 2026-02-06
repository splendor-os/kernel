//! # Filesystem Adapter
//!
//! Provides safe, sandboxed filesystem operations behind the action gateway.

use splendor_gateway::{ActionAdapter, ActionRequest, AdapterError, AdapterResult};
use splendor_types::TenantId;
use std::fs;
use std::path::{Component, Path, PathBuf};

const READ_ACTION: &str = "read_file";
const WRITE_ACTION: &str = "write_file";
const LIST_ACTION: &str = "list_dir";
const STAT_ACTION: &str = "stat";

/// Configuration for the filesystem adapter.
#[derive(Clone, Debug)]
pub struct FilesystemAdapterConfig {
    /// Base directory used to create per-tenant sandboxes.
    pub base_dir: PathBuf,
    /// Maximum bytes allowed for a single read.
    pub max_read_bytes: u64,
    /// Maximum bytes allowed for a single write.
    pub max_write_bytes: u64,
    /// Maximum directory entries returned by `list_dir`.
    pub max_list_entries: usize,
}

impl Default for FilesystemAdapterConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("splendor-data"),
            max_read_bytes: 1024 * 1024,
            max_write_bytes: 1024 * 1024,
            max_list_entries: 256,
        }
    }
}

/// Filesystem adapter implementing sandboxed file operations.
#[derive(Clone, Debug)]
pub struct FilesystemAdapter {
    config: FilesystemAdapterConfig,
}

impl FilesystemAdapter {
    /// Creates a new filesystem adapter with the provided configuration.
    pub fn new(config: FilesystemAdapterConfig) -> Self {
        Self { config }
    }

    fn tenant_root(&self, tenant_id: &TenantId) -> Result<PathBuf, AdapterError> {
        let root = self.config.base_dir.join(tenant_id.to_string());
        fs::create_dir_all(&root).map_err(|error| AdapterError::Failed(error.to_string()))?;
        Ok(root)
    }

    fn resolve_path(&self, root: &Path, raw: &str) -> Result<PathBuf, AdapterError> {
        let path = Path::new(raw);
        if path.is_absolute() {
            return Err(AdapterError::Failed(
                "absolute paths are not allowed".to_string(),
            ));
        }
        let mut clean = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(segment) => clean.push(segment),
                Component::CurDir => {}
                _ => {
                    return Err(AdapterError::Failed(
                        "path traversal outside sandbox is not allowed".to_string(),
                    ))
                }
            }
        }
        Ok(root.join(clean))
    }

    fn param_string<'a>(
        &self,
        params: &'a serde_json::Value,
        key: &str,
    ) -> Result<&'a str, AdapterError> {
        params
            .get(key)
            .and_then(|value| value.as_str())
            .ok_or_else(|| AdapterError::Failed(format!("missing or invalid {key}")))
    }

    fn param_bytes(&self, params: &serde_json::Value) -> Result<Vec<u8>, AdapterError> {
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
            return Ok(output);
        }

        if let Some(contents) = params.get("contents") {
            let text = contents
                .as_str()
                .ok_or_else(|| AdapterError::Failed("contents must be a string".to_string()))?;
            return Ok(text.as_bytes().to_vec());
        }

        Err(AdapterError::Failed(
            "missing contents or bytes parameter".to_string(),
        ))
    }

    fn read_file(&self, path: &Path) -> Result<AdapterResult, AdapterError> {
        let metadata =
            fs::metadata(path).map_err(|error| AdapterError::Failed(error.to_string()))?;
        let size = metadata.len();
        if size > self.config.max_read_bytes {
            return Err(AdapterError::Failed(format!(
                "read size {size} exceeds limit {}",
                self.config.max_read_bytes
            )));
        }
        let bytes = fs::read(path).map_err(|error| AdapterError::Failed(error.to_string()))?;
        let output = serde_json::json!({
            "path": path.to_string_lossy(),
            "bytes": bytes,
            "bytes_read": size,
        });
        Ok(AdapterResult {
            output,
            satisfied_postconditions: Vec::new(),
        })
    }

    fn write_file(
        &self,
        path: &Path,
        params: &serde_json::Value,
    ) -> Result<AdapterResult, AdapterError> {
        let bytes = self.param_bytes(params)?;
        let size = bytes.len() as u64;
        if size > self.config.max_write_bytes {
            return Err(AdapterError::Failed(format!(
                "write size {size} exceeds limit {}",
                self.config.max_write_bytes
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| AdapterError::Failed(error.to_string()))?;
        }
        fs::write(path, &bytes).map_err(|error| AdapterError::Failed(error.to_string()))?;
        let output = serde_json::json!({
            "path": path.to_string_lossy(),
            "bytes_written": size,
        });
        Ok(AdapterResult {
            output,
            satisfied_postconditions: Vec::new(),
        })
    }

    fn list_dir(&self, path: &Path) -> Result<AdapterResult, AdapterError> {
        let mut entries = Vec::new();
        let mut truncated = false;
        for entry in fs::read_dir(path).map_err(|error| AdapterError::Failed(error.to_string()))? {
            let entry = entry.map_err(|error| AdapterError::Failed(error.to_string()))?;
            let metadata = entry
                .metadata()
                .map_err(|error| AdapterError::Failed(error.to_string()))?;
            entries.push(serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "is_dir": metadata.is_dir(),
                "is_file": metadata.is_file(),
                "size": metadata.len(),
            }));
            if entries.len() >= self.config.max_list_entries {
                truncated = true;
                break;
            }
        }
        let output = serde_json::json!({
            "path": path.to_string_lossy(),
            "entries": entries,
            "truncated": truncated,
        });
        Ok(AdapterResult {
            output,
            satisfied_postconditions: Vec::new(),
        })
    }

    fn stat_file(&self, path: &Path) -> Result<AdapterResult, AdapterError> {
        let metadata =
            fs::metadata(path).map_err(|error| AdapterError::Failed(error.to_string()))?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        let output = serde_json::json!({
            "path": path.to_string_lossy(),
            "is_dir": metadata.is_dir(),
            "is_file": metadata.is_file(),
            "size": metadata.len(),
            "modified_at": modified,
        });
        Ok(AdapterResult {
            output,
            satisfied_postconditions: Vec::new(),
        })
    }
}

impl ActionAdapter for FilesystemAdapter {
    fn execute(&self, action: &ActionRequest) -> Result<AdapterResult, AdapterError> {
        let params = action
            .action
            .params
            .as_object()
            .ok_or_else(|| AdapterError::Failed("params must be an object".to_string()))?;
        let action_name = action.action.name.as_str();

        let mut result = match action_name {
            READ_ACTION => {
                let root = self.tenant_root(&action.tenant_id)?;
                let path =
                    self.resolve_path(&root, self.param_string(&action.action.params, "path")?)?;
                self.read_file(&path)?
            }
            WRITE_ACTION => {
                let root = self.tenant_root(&action.tenant_id)?;
                let path =
                    self.resolve_path(&root, self.param_string(&action.action.params, "path")?)?;
                self.write_file(&path, &action.action.params)?
            }
            LIST_ACTION => {
                let root = self.tenant_root(&action.tenant_id)?;
                let raw_path = params
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or(".");
                let path = self.resolve_path(&root, raw_path)?;
                self.list_dir(&path)?
            }
            STAT_ACTION => {
                let root = self.tenant_root(&action.tenant_id)?;
                let path =
                    self.resolve_path(&root, self.param_string(&action.action.params, "path")?)?;
                self.stat_file(&path)?
            }
            other => {
                return Err(AdapterError::Failed(format!(
                    "unsupported filesystem action {other}"
                )))
            }
        };
        result.satisfied_postconditions = action.action.postconditions.clone();
        Ok(result)
    }
}

#[cfg(test)]
#[path = "../tests/unit/filesystem_tests.rs"]
mod tests;
