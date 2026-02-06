//! # Scheduler
//!
//! A cooperative scheduler that executes agent loop engines in a fair queue and
//! enforces a per-tick time budget. Each scheduler cycle resets tenant quota
//! ledgers so limits apply across all agents in the tenant.

use crate::loop_engine::{LoopEngine, LoopError, TickOutcome};
use crate::tenancy::TenantRegistry;
use splendor_types::{AgentId, TenantId};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use time::OffsetDateTime;

/// Scheduler configuration options.
#[derive(Clone, Debug, Default)]
pub struct SchedulerConfig {
    /// Optional per-tick time budget.
    pub tick_budget: Option<Duration>,
    /// Optional tick interval to enforce predictable boundaries.
    pub tick_interval: Option<Duration>,
}

/// Result of running a scheduler step.
#[derive(Clone, Debug)]
pub struct SchedulerStep {
    /// Tick identifier assigned by the scheduler.
    pub tick_id: u64,
    /// Agent identifier that was executed.
    pub agent_id: AgentId,
    /// Outcome produced by the loop engine.
    pub outcome: TickOutcome,
    /// Wall-clock duration for the tick.
    pub elapsed: Duration,
}

/// Scheduler errors.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    /// No agents are registered with the scheduler.
    #[error("no agents registered")]
    NoAgents,
    /// No tenant context was found for the agent.
    #[error("tenant context was not found for tenant {0}")]
    MissingTenant(TenantId),
    /// A loop engine returned an error.
    #[error("loop engine failed: {0}")]
    Loop(#[from] LoopError),
    /// Tick budget exceeded for the executed step.
    #[error("tick budget exceeded ({elapsed:?} > {budget:?})")]
    TickBudgetExceeded {
        /// Step that exceeded the budget.
        step: Box<SchedulerStep>,
        /// Budget configured for the scheduler.
        budget: Duration,
        /// Observed elapsed duration.
        elapsed: Duration,
    },
}

/// Cooperative scheduler for agent loop engines.
pub struct Scheduler {
    config: SchedulerConfig,
    tenants: TenantRegistry,
    queue: VecDeque<LoopEngine>,
    tick_id: u64,
    cycle_remaining: usize,
}

impl Scheduler {
    /// Creates a new scheduler with the provided config.
    pub fn new(config: SchedulerConfig) -> Self {
        Self::with_registry(config, TenantRegistry::new())
    }

    /// Creates a scheduler backed by an explicit tenant registry.
    pub fn with_registry(config: SchedulerConfig, registry: TenantRegistry) -> Self {
        Self {
            config,
            tenants: registry,
            queue: VecDeque::new(),
            tick_id: 0,
            cycle_remaining: 0,
        }
    }

    /// Returns a clone of the tenant registry.
    pub fn tenant_registry(&self) -> TenantRegistry {
        self.tenants.clone()
    }

    /// Registers a tenant context with the scheduler.
    pub fn register_tenant(&mut self, tenant: crate::TenantContext) {
        self.tenants.insert(tenant);
    }

    /// Returns tick usage for a tenant if available.
    pub fn tenant_usage(&self, tenant_id: &TenantId) -> Option<crate::QuotaUsage> {
        self.tenants
            .with_tenant(tenant_id, |tenant| tenant.tick_usage())
    }

    /// Adds a loop engine to the scheduling queue.
    pub fn add_agent(&mut self, engine: LoopEngine) {
        self.queue.push_back(engine);
    }

    /// Runs a single agent tick.
    pub fn run_once(&mut self) -> Result<SchedulerStep, SchedulerError> {
        if self.queue.is_empty() {
            return Err(SchedulerError::NoAgents);
        }
        self.ensure_cycle();

        let mut engine = self.queue.pop_front().expect("queue not empty");
        let tenant_id = engine.tenant_id().clone();
        if self.tenants.with_tenant(&tenant_id, |_| ()).is_none() {
            self.queue.push_back(engine);
            self.cycle_remaining = self.cycle_remaining.saturating_sub(1);
            return Err(SchedulerError::MissingTenant(tenant_id));
        }

        let start = Instant::now();
        let outcome = match engine.tick(self.tick_id) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.queue.push_back(engine);
                self.cycle_remaining = self.cycle_remaining.saturating_sub(1);
                return Err(SchedulerError::Loop(error));
            }
        };
        let elapsed = start.elapsed();

        let step = SchedulerStep {
            tick_id: self.tick_id,
            agent_id: engine.agent_id().clone(),
            outcome,
            elapsed,
        };

        self.queue.push_back(engine);
        self.cycle_remaining = self.cycle_remaining.saturating_sub(1);

        if let Some(budget) = self.config.tick_budget {
            if elapsed > budget {
                return Err(SchedulerError::TickBudgetExceeded {
                    step: Box::new(step),
                    budget,
                    elapsed,
                });
            }
        }

        Ok(step)
    }

    /// Runs a full scheduler cycle (each agent once).
    pub fn run_cycle(&mut self) -> Result<Vec<SchedulerStep>, SchedulerError> {
        if self.queue.is_empty() {
            return Err(SchedulerError::NoAgents);
        }
        let cycle_start = Instant::now();
        let remaining = self.queue.len();
        let mut steps = Vec::with_capacity(remaining);
        for _ in 0..remaining {
            steps.push(self.run_once()?);
        }
        self.enforce_tick_interval(cycle_start);
        Ok(steps)
    }

    /// Runs the scheduler for a fixed number of cycles.
    pub fn run_cycles(&mut self, cycles: u64) -> Result<Vec<SchedulerStep>, SchedulerError> {
        let mut steps = Vec::new();
        for _ in 0..cycles {
            steps.extend(self.run_cycle()?);
        }
        Ok(steps)
    }

    /// Runs the scheduler continuously until an error occurs.
    pub fn run_forever(&mut self) -> Result<(), SchedulerError> {
        loop {
            self.run_cycle()?;
        }
    }

    fn ensure_cycle(&mut self) {
        if self.cycle_remaining > 0 {
            return;
        }
        self.tick_id = self.tick_id.saturating_add(1);
        let now = OffsetDateTime::now_utc();
        self.tenants.begin_tick(self.tick_id, now);
        self.cycle_remaining = self.queue.len();
    }

    fn enforce_tick_interval(&self, cycle_start: Instant) {
        if let Some(interval) = self.config.tick_interval {
            let elapsed = cycle_start.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/scheduler_tests.rs"]
mod tests;
