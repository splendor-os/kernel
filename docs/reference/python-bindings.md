# Python Bindings

The Python bindings expose a thin PyO3 wrapper around the Python SDK runtime.
They allow Python users to construct and control a `KernelRuntime` instance
through a compiled extension module.

## splendor_bindings module

When built with the `python` feature, the module exports:

- `KernelRuntime`: wrapper over `splendor.runtime.KernelRuntime`.
- `KernelRuntimeConfig`: Python config class.
- `QuotaPolicy`: Python quota policy class.
- `__version__`: package version string.

## Build

From `python/bindings`:

```
maturin develop --features python
```

This installs the extension module into your active Python environment.

## Example

```python
import splendor_bindings

runtime = splendor_bindings.KernelRuntime()
tenant_id = runtime.create_tenant(
    allowed_actions=["noop"],
    allowed_adapters=["noop"],
)
agent_id = runtime.create_agent(tenant_id)

runtime.register_adapter("noop", lambda action: {"output": {"ok": True}})
runtime.register_perceptor(agent_id, lambda agent: [])
runtime.register_policy(
    agent_id,
    lambda state, percepts: [
        {
            "name": "noop",
            "params": {},
            "side_effect_class": "read_only",
            "adapter": "noop",
        }
    ],
)

runtime.run_once(agent_id)
print(splendor_bindings.__version__)
```
