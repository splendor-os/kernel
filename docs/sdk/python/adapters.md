# Python SDK Adapters

Adapters are callbacks registered by adapter ID and invoked only after the local
runtime verifies the action candidate.

## Register an adapter

```python
from splendor import Action

def local_adapter(action: Action) -> dict[str, object]:
    return {"output": {"handled": action.name}, "satisfied_postconditions": []}

runtime.register_adapter("local", local_adapter)
```

## Allowed action

The tenant must allow the action name and adapter ID:

```python
tenant_id = runtime.create_tenant(
    allowed_actions=["allowed"],
    allowed_adapters=["local"],
)
```

An allowed candidate executes after checks pass and returns status `executed`.

## Denied action

If the action name, adapter ID, required permission, quota, precondition, or
constraint fails, the SDK records status `denied` and does not call the adapter.

## Failed action

If verification passes but no adapter is registered, or the adapter raises an
exception, the SDK records status `failed` and preserves the error string.

## Gateway boundary rule

Official SDK examples never call adapter callbacks directly for side effects.
Policy code returns action candidates; `KernelRuntime.run_once` is the execution
boundary.
