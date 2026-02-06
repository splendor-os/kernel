//! # Hello Kernel Example
//!
//! Demonstrates booting the kernel runtime and emitting a simple trace event.
//!
//! ## Running
//! ```bash
//! cargo run --example hello_kernel
//! ```

use splendor_kernel::{KernelRuntime, KernelRuntimeConfig, TraceEventKind};

/// Boots the kernel runtime and emits a policy invocation event.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = KernelRuntime::boot(KernelRuntimeConfig::default())?;
    runtime.record_event(TraceEventKind::PolicyInvoked {
        policy: "hello_kernel".to_string(),
    })?;
    Ok(())
}
