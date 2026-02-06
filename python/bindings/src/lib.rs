//! # Python Bindings Entry Point
//!
//! Minimal PyO3 module that exposes version metadata for the Splendor Python
//! package while ensuring the Rust kernel crate is linked.
//!
//! ## Example
//! ```rust
//! use splendor_bindings::bindings_version;
//!
//! assert!(!bindings_version().is_empty());
//! ```

/// Returns the compiled package version and exercises the kernel config type.
pub fn bindings_version() -> &'static str {
    let _config = splendor_kernel::KernelRuntimeConfig::default();
    env!("CARGO_PKG_VERSION")
}

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::{PyDict, PyTuple};

#[cfg(feature = "python")]
#[pyclass]
struct KernelRuntime {
    inner: Py<PyAny>,
}

#[cfg(feature = "python")]
#[pymethods]
impl KernelRuntime {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(py: Python<'_>, config: Option<PyObject>) -> PyResult<Self> {
        let module = py.import("splendor.runtime")?;
        let class = module.getattr("KernelRuntime")?;
        let instance = if let Some(config) = config {
            class.call1(PyTuple::new(py, [config]))?
        } else {
            class.call0()?
        };
        Ok(Self {
            inner: instance.into_py(py),
        })
    }

    #[pyo3(signature = (*, tenant_id=None, allowed_actions=None, allowed_adapters=None, quotas=None))]
    fn create_tenant(
        &self,
        py: Python<'_>,
        tenant_id: Option<String>,
        allowed_actions: Option<Vec<String>>,
        allowed_adapters: Option<Vec<String>>,
        quotas: Option<PyObject>,
    ) -> PyResult<String> {
        let kwargs = PyDict::new(py);
        if let Some(tenant_id) = tenant_id {
            kwargs.set_item("tenant_id", tenant_id)?;
        }
        if let Some(actions) = allowed_actions {
            kwargs.set_item("allowed_actions", actions)?;
        }
        if let Some(adapters) = allowed_adapters {
            kwargs.set_item("allowed_adapters", adapters)?;
        }
        if let Some(quotas) = quotas {
            kwargs.set_item("quotas", quotas)?;
        }
        let result = self
            .inner
            .call_method(py, "create_tenant", (), Some(kwargs))?;
        result.extract(py)
    }

    #[pyo3(signature = (tenant_id, *, state=None, content_type=None, snapshot_interval=None, run_id=None))]
    fn create_agent(
        &self,
        py: Python<'_>,
        tenant_id: String,
        state: Option<Vec<u8>>,
        content_type: Option<String>,
        snapshot_interval: Option<u64>,
        run_id: Option<String>,
    ) -> PyResult<String> {
        let kwargs = PyDict::new(py);
        if let Some(state) = state {
            kwargs.set_item("state", state)?;
        }
        if let Some(content_type) = content_type {
            kwargs.set_item("content_type", content_type)?;
        }
        if let Some(interval) = snapshot_interval {
            kwargs.set_item("snapshot_interval", interval)?;
        }
        if let Some(run_id) = run_id {
            kwargs.set_item("run_id", run_id)?;
        }
        let result = self
            .inner
            .call_method(py, "create_agent", (tenant_id,), Some(kwargs))?;
        result.extract(py)
    }

    fn register_perceptor(
        &self,
        py: Python<'_>,
        agent_id: String,
        perceptor: PyObject,
    ) -> PyResult<()> {
        self.inner
            .call_method(py, "register_perceptor", (agent_id, perceptor), None)?;
        Ok(())
    }

    fn register_policy(&self, py: Python<'_>, agent_id: String, policy: PyObject) -> PyResult<()> {
        self.inner
            .call_method(py, "register_policy", (agent_id, policy), None)?;
        Ok(())
    }

    fn register_constraints(
        &self,
        py: Python<'_>,
        agent_id: String,
        constraints: PyObject,
    ) -> PyResult<()> {
        self.inner
            .call_method(py, "register_constraints", (agent_id, constraints), None)?;
        Ok(())
    }

    fn register_adapter(
        &self,
        py: Python<'_>,
        adapter_id: String,
        adapter: PyObject,
    ) -> PyResult<()> {
        self.inner
            .call_method(py, "register_adapter", (adapter_id, adapter), None)?;
        Ok(())
    }

    #[pyo3(signature = (agent_id, tick_interval=None))]
    fn start(&self, py: Python<'_>, agent_id: String, tick_interval: Option<f64>) -> PyResult<()> {
        let kwargs = PyDict::new(py);
        if let Some(interval) = tick_interval {
            kwargs.set_item("tick_interval", interval)?;
        }
        self.inner
            .call_method(py, "start", (agent_id,), Some(kwargs))?;
        Ok(())
    }

    fn stop(&self, py: Python<'_>, agent_id: String) -> PyResult<()> {
        self.inner.call_method(py, "stop", (agent_id,), None)?;
        Ok(())
    }

    fn run_once(&self, py: Python<'_>, agent_id: String) -> PyResult<PyObject> {
        let result = self.inner.call_method(py, "run_once", (agent_id,), None)?;
        Ok(result.into_py(py))
    }

    fn subscribe_traces(&self, py: Python<'_>, run_id: String, callback: PyObject) -> PyResult<()> {
        self.inner
            .call_method(py, "subscribe_traces", (run_id, callback), None)?;
        Ok(())
    }

    fn tail_traces(&self, py: Python<'_>, run_id: String) -> PyResult<PyObject> {
        let result = self.inner.call_method(py, "tail_traces", (run_id,), None)?;
        Ok(result.into_py(py))
    }

    fn agent_run_id(&self, py: Python<'_>, agent_id: String) -> PyResult<String> {
        let result = self
            .inner
            .call_method(py, "agent_run_id", (agent_id,), None)?;
        result.extract(py)
    }

    fn agent_state(&self, py: Python<'_>, agent_id: String) -> PyResult<Vec<u8>> {
        let result = self
            .inner
            .call_method(py, "agent_state", (agent_id,), None)?;
        result.extract(py)
    }

    fn agent_tick(&self, py: Python<'_>, agent_id: String) -> PyResult<u64> {
        let result = self
            .inner
            .call_method(py, "agent_tick", (agent_id,), None)?;
        result.extract(py)
    }
}

#[cfg(feature = "python")]
/// PyO3 module initializer that exposes the runtime wrapper.
#[pymodule]
fn splendor_bindings(py: Python<'_>, module: &PyModule) -> PyResult<()> {
    module.add_class::<KernelRuntime>()?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("__doc__", "Splendor kernel bindings")?;
    let runtime_module = py.import("splendor.runtime")?;
    if let Ok(config) = runtime_module.getattr("KernelRuntimeConfig") {
        module.add("KernelRuntimeConfig", config)?;
    }
    if let Ok(policy) = runtime_module.getattr("QuotaPolicy") {
        module.add("QuotaPolicy", policy)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_version_is_not_empty() {
        let version = bindings_version();
        assert!(!version.is_empty());
    }
}
