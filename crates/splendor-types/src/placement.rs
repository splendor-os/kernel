//! # Placement v0 Contract
//!
//! Deterministic placement primitives for 0.03-S4. The matcher selects one
//! already-registered runtime target from declared capabilities, locality,
//! runtime-version compatibility, and dedicated-instance availability. It does
//! not validate signed work-order authority, perform autoscaling, or execute any
//! side effects.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Canonical schema marker for serialized placement decisions.
pub const PLACEMENT_DECISION_SCHEMA: &str = "splendor.placement.decision.v1";

/// Execution target class requested by a work order or selected by placement.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementTarget {
    /// Ephemeral cloud runtime started for one or a small number of runs.
    EphemeralCloud,
    /// Resident cloud worker pool.
    ResidentCloudPool,
    /// Customer VPC runtime boundary.
    CustomerVpc,
    /// On-prem runtime near private systems.
    OnPrem,
    /// Edge appliance or non-robot edge runtime.
    EdgeDevice,
    /// Resident runtime on a physical robot/drone/humanoid device.
    PhysicalRobot,
    /// Desktop sidecar runtime.
    DesktopSidecar,
}

impl PlacementTarget {
    /// Returns the canonical external target string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EphemeralCloud => "ephemeral_cloud",
            Self::ResidentCloudPool => "resident_cloud_pool",
            Self::CustomerVpc => "customer_vpc",
            Self::OnPrem => "on_prem",
            Self::EdgeDevice => "edge_device",
            Self::PhysicalRobot => "physical_robot",
            Self::DesktopSidecar => "desktop_sidecar",
        }
    }

    fn is_cloud(self) -> bool {
        matches!(self, Self::EphemeralCloud | Self::ResidentCloudPool)
    }
}

/// Runtime-level data locality used for placement hints and audit output.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataLocality {
    /// General cloud-local data or compute.
    Cloud,
    /// Customer VPC-local data or compute.
    Vpc,
    /// On-premises data or compute.
    OnPrem,
    /// Device-local data or compute.
    Device,
}

impl DataLocality {
    /// Returns the canonical external locality string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::Vpc => "vpc",
            Self::OnPrem => "on_prem",
            Self::Device => "device",
        }
    }
}

/// Execution intent used to prevent accidental physical authority escalation.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementExecutionMode {
    /// Live workload on the selected runtime target.
    #[default]
    Live,
    /// Safe simulation of a physical request on non-physical compute.
    Simulation,
    /// Cloud/helper computation that can propose plans but not execute actuators.
    CloudHelper,
}

impl PlacementExecutionMode {
    /// Returns the canonical external execution mode string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Simulation => "simulation",
            Self::CloudHelper => "cloud_helper",
        }
    }
}

/// Request-side placement requirements supplied after work-order authority has
/// already been validated by the work-order layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlacementRequest {
    /// Requested target class.
    pub target: PlacementTarget,
    /// Capabilities required by the run. Empty or whitespace-only capability
    /// tokens fail closed.
    pub required_capabilities: Vec<String>,
    /// Data locality hint/requirement to preserve in decisions and audit output.
    pub data_locality: Option<DataLocality>,
    /// Whether a dedicated instance/isolation boundary is required.
    pub dedicated_instance: bool,
    /// Optional exact runtime version required by this request.
    pub required_runtime_version: Option<String>,
    /// Optional maximum runtime propagated into the decision contract.
    pub max_runtime_ms: Option<u64>,
    /// Execution intent, especially for physical requests that may be simulated
    /// or assisted by cloud compute without granting actuator authority.
    pub execution_mode: PlacementExecutionMode,
}

impl PlacementRequest {
    /// Builds a minimal placement request for a target class.
    pub fn new(target: PlacementTarget) -> Self {
        Self {
            target,
            required_capabilities: Vec::new(),
            data_locality: None,
            dedicated_instance: false,
            required_runtime_version: None,
            max_runtime_ms: None,
            execution_mode: PlacementExecutionMode::Live,
        }
    }

    fn validation_reasons(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        if has_blank_tokens(&self.required_capabilities) {
            reasons.push("required capabilities must not contain blank tokens".to_string());
        }
        if self
            .required_runtime_version
            .as_deref()
            .map(str::trim)
            .is_some_and(str::is_empty)
        {
            reasons.push("required runtime version must not be blank".to_string());
        }
        reasons
    }

    fn normalized_required_capabilities(&self) -> Vec<String> {
        normalize_tokens(&self.required_capabilities)
    }
}

/// Candidate runtime target discovered from a node/instance registry or fixture.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlacementCandidate {
    /// Opaque candidate reference. Later registry work can map this to concrete
    /// node and instance identities without changing the decision contract.
    pub candidate_id: String,
    /// Candidate target class.
    pub target: PlacementTarget,
    /// Declared capabilities advertised by this candidate.
    pub capabilities: Vec<String>,
    /// Candidate data locality.
    pub data_locality: Option<DataLocality>,
    /// Runtime version reported by the candidate.
    pub runtime_version: String,
    /// Whether this candidate can satisfy a dedicated-instance request.
    pub dedicated_instance_available: bool,
    /// Whether this candidate is currently available for new placement.
    pub available: bool,
    /// Execution modes supported by this candidate.
    pub supported_execution_modes: Vec<PlacementExecutionMode>,
}

impl PlacementCandidate {
    /// Builds a placement candidate with safe shared/live defaults.
    pub fn new(
        candidate_id: impl Into<String>,
        target: PlacementTarget,
        capabilities: Vec<String>,
        runtime_version: impl Into<String>,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            target,
            capabilities,
            data_locality: None,
            runtime_version: runtime_version.into(),
            dedicated_instance_available: false,
            available: true,
            supported_execution_modes: vec![PlacementExecutionMode::Live],
        }
    }

    fn validation_reasons(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        if self.candidate_id.trim().is_empty() {
            reasons.push("candidate_id is required".to_string());
        }
        if has_blank_tokens(&self.capabilities) {
            reasons.push("candidate capabilities must not contain blank tokens".to_string());
        }
        if self.runtime_version.trim().is_empty() {
            reasons.push("candidate runtime version is required".to_string());
        }
        if self.supported_execution_modes.is_empty() {
            reasons.push("candidate must declare at least one execution mode".to_string());
        }
        reasons
    }

    fn normalized_capabilities(&self) -> BTreeSet<String> {
        normalize_tokens(&self.capabilities).into_iter().collect()
    }
}

/// Top-level placement outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementDecisionStatus {
    /// A candidate was selected.
    Selected,
    /// No candidate satisfied the request. The caller must not silently broaden
    /// capabilities, target class, permissions, or work-order authority.
    Rejected,
}

/// Structured rejection detail for explaining placement decisions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementRejectionReason {
    /// The request itself was malformed or underspecified.
    InvalidRequest { reason: String },
    /// A candidate advertised malformed capability/runtime metadata.
    InvalidCandidate {
        candidate_id: String,
        reason: String,
    },
    /// No placement candidates were supplied.
    NoCandidates,
    /// Candidate was not available.
    CandidateUnavailable { candidate_id: String },
    /// Candidate target class does not match the requested target class.
    TargetMismatch {
        candidate_id: String,
        requested: PlacementTarget,
        candidate: PlacementTarget,
    },
    /// A live physical request attempted to land on non-physical cloud compute.
    PhysicalRequiresDeviceOrExplicitHelper {
        candidate_id: String,
        candidate: PlacementTarget,
    },
    /// Candidate cannot support the requested execution mode.
    UnsupportedExecutionMode {
        candidate_id: String,
        mode: PlacementExecutionMode,
    },
    /// Candidate is missing a required capability.
    MissingCapability {
        candidate_id: String,
        capability: String,
    },
    /// Candidate runtime version does not satisfy the request.
    IncompatibleRuntime {
        candidate_id: String,
        required: String,
        found: String,
    },
    /// Candidate locality does not satisfy the request.
    DataLocalityMismatch {
        candidate_id: String,
        required: DataLocality,
        found: Option<DataLocality>,
    },
    /// Candidate cannot provide a requested dedicated instance.
    DedicatedInstanceRequired { candidate_id: String },
}

impl PlacementRejectionReason {
    fn reason_text(&self) -> String {
        match self {
            Self::InvalidRequest { reason } => format!("invalid placement request: {reason}"),
            Self::InvalidCandidate {
                candidate_id,
                reason,
            } => format!("candidate `{candidate_id}` is invalid: {reason}"),
            Self::NoCandidates => "no placement candidates were supplied".to_string(),
            Self::CandidateUnavailable { candidate_id } => {
                format!("candidate `{candidate_id}` is not available")
            }
            Self::TargetMismatch {
                candidate_id,
                requested,
                candidate,
            } => format!(
                "candidate `{candidate_id}` target `{}` does not match requested target `{}`",
                candidate.as_str(),
                requested.as_str()
            ),
            Self::PhysicalRequiresDeviceOrExplicitHelper {
                candidate_id,
                candidate,
            } => format!(
                "candidate `{candidate_id}` target `{}` cannot run a live physical request without explicit simulation or cloud_helper mode",
                candidate.as_str()
            ),
            Self::UnsupportedExecutionMode { candidate_id, mode } => format!(
                "candidate `{candidate_id}` does not support execution mode `{}`",
                mode.as_str()
            ),
            Self::MissingCapability {
                candidate_id,
                capability,
            } => format!("candidate `{candidate_id}` is missing capability `{capability}`"),
            Self::IncompatibleRuntime {
                candidate_id,
                required,
                found,
            } => format!(
                "candidate `{candidate_id}` runtime `{found}` does not satisfy required runtime `{required}`"
            ),
            Self::DataLocalityMismatch {
                candidate_id,
                required,
                found,
            } => format!(
                "candidate `{candidate_id}` data locality `{}` does not satisfy required locality `{}`",
                display_locality(*found),
                required.as_str()
            ),
            Self::DedicatedInstanceRequired { candidate_id } => format!(
                "candidate `{candidate_id}` cannot provide the required dedicated instance"
            ),
        }
    }
}

/// Per-candidate explanation generated by the deterministic matcher.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlacementCandidateEvaluation {
    /// Evaluated candidate reference.
    pub candidate_id: String,
    /// Candidate target class.
    pub target: PlacementTarget,
    /// Whether this candidate satisfied all request requirements.
    pub accepted: bool,
    /// Human-readable rejection or selection reasons.
    pub reasons: Vec<String>,
    /// Structured rejection reasons for deterministic replay/explanation.
    pub rejection_reasons: Vec<PlacementRejectionReason>,
}

/// Audit payload that a management trace/audit sink can persist without
/// re-running placement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlacementTraceAudit {
    /// Schema of this audit payload.
    pub schema: String,
    /// Requested target class.
    pub requested_target: PlacementTarget,
    /// Selected target class, if placement succeeded.
    pub selected_target: Option<PlacementTarget>,
    /// Selected candidate reference, if placement succeeded.
    pub selected_candidate_id: Option<String>,
    /// Request execution mode.
    pub execution_mode: PlacementExecutionMode,
    /// Dedicated-instance requirement preserved for audit/replay.
    pub dedicated_instance: bool,
    /// Required capabilities preserved for audit/replay.
    pub required_capabilities: Vec<String>,
    /// Data-locality hint/requirement preserved for audit/replay.
    pub data_locality: Option<DataLocality>,
    /// Decision reasons preserved for audit/replay.
    pub reasons: Vec<String>,
}

/// Explain output for the complete placement decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlacementExplain {
    /// Requested target class.
    pub requested_target: PlacementTarget,
    /// Request execution mode.
    pub execution_mode: PlacementExecutionMode,
    /// Candidate evaluations in deterministic order.
    pub evaluated_candidates: Vec<PlacementCandidateEvaluation>,
}

/// Deterministic placement decision contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlacementDecision {
    /// Selected or rejected outcome.
    pub status: PlacementDecisionStatus,
    /// Selected target class on success, requested target class on rejection.
    pub target: PlacementTarget,
    /// Selected candidate reference, if placement succeeded.
    pub candidate_id: Option<String>,
    /// Human-readable decision reasons.
    pub reasons: Vec<String>,
    /// Dedicated-instance requirement copied from the request.
    pub dedicated_instance: bool,
    /// Required capabilities copied from the request in deterministic order.
    pub required_capabilities: Vec<String>,
    /// Data-locality hint/requirement copied from the request.
    pub data_locality: Option<DataLocality>,
    /// Optional runtime budget copied from the request.
    pub max_runtime_ms: Option<u64>,
    /// Structured candidate-by-candidate explain output.
    pub explain: PlacementExplain,
    /// Trace/audit payload that preserves placement evidence.
    pub trace_audit: PlacementTraceAudit,
}

/// Selects one placement candidate deterministically or returns an explicit
/// rejection decision. The matcher is deliberately simple: target compatibility,
/// runtime compatibility, locality, required capabilities, and dedicated
/// availability must all pass before a candidate can be selected.
pub fn select_placement(
    request: &PlacementRequest,
    candidates: &[PlacementCandidate],
) -> PlacementDecision {
    let required_capabilities = request.normalized_required_capabilities();
    let request_rejections: Vec<_> = request
        .validation_reasons()
        .into_iter()
        .map(|reason| PlacementRejectionReason::InvalidRequest { reason })
        .collect();

    if !request_rejections.is_empty() {
        let reasons = reason_texts(&request_rejections);
        return build_decision(
            request,
            request.target,
            None,
            PlacementDecisionStatus::Rejected,
            reasons,
            required_capabilities,
            Vec::new(),
        );
    }

    if candidates.is_empty() {
        let rejection = PlacementRejectionReason::NoCandidates;
        return build_decision(
            request,
            request.target,
            None,
            PlacementDecisionStatus::Rejected,
            vec![rejection.reason_text()],
            required_capabilities,
            Vec::new(),
        );
    }

    let mut ordered_candidates = candidates.to_vec();
    ordered_candidates.sort_by(|left, right| {
        (
            left.target,
            left.data_locality,
            left.candidate_id.as_str(),
            left.runtime_version.as_str(),
        )
            .cmp(&(
                right.target,
                right.data_locality,
                right.candidate_id.as_str(),
                right.runtime_version.as_str(),
            ))
    });

    let mut evaluations = Vec::with_capacity(ordered_candidates.len());
    for candidate in &ordered_candidates {
        let evaluation = evaluate_candidate(request, &required_capabilities, candidate);
        if evaluation.accepted {
            let mut reasons = vec![format!(
                "selected candidate `{}` for target `{}`",
                candidate.candidate_id,
                candidate.target.as_str()
            )];
            if let Some(locality) = request.data_locality {
                reasons.push(format!(
                    "preserved data locality `{}` in placement decision and audit output",
                    locality.as_str()
                ));
            }
            if request.dedicated_instance {
                reasons.push("candidate satisfies dedicated-instance requirement".to_string());
            }
            if request.target == PlacementTarget::PhysicalRobot && candidate.target.is_cloud() {
                reasons.push(format!(
                    "physical request explicitly marked as `{}`; no live actuator authority granted to cloud target",
                    request.execution_mode.as_str()
                ));
            }
            evaluations.push(evaluation);
            return build_decision(
                request,
                candidate.target,
                Some(candidate.candidate_id.clone()),
                PlacementDecisionStatus::Selected,
                reasons,
                required_capabilities,
                evaluations,
            );
        }
        evaluations.push(evaluation);
    }

    let mut reasons = vec![
        "no placement candidate satisfied target, capability, locality, runtime, and dedicated-instance requirements"
            .to_string(),
    ];
    reasons.extend(
        evaluations
            .iter()
            .flat_map(|evaluation| evaluation.reasons.iter().cloned()),
    );
    reasons.sort();
    reasons.dedup();

    build_decision(
        request,
        request.target,
        None,
        PlacementDecisionStatus::Rejected,
        reasons,
        required_capabilities,
        evaluations,
    )
}

fn evaluate_candidate(
    request: &PlacementRequest,
    required_capabilities: &[String],
    candidate: &PlacementCandidate,
) -> PlacementCandidateEvaluation {
    let mut rejection_reasons = Vec::new();

    for reason in candidate.validation_reasons() {
        rejection_reasons.push(PlacementRejectionReason::InvalidCandidate {
            candidate_id: candidate.candidate_id.clone(),
            reason,
        });
    }

    if !candidate.available {
        rejection_reasons.push(PlacementRejectionReason::CandidateUnavailable {
            candidate_id: candidate.candidate_id.clone(),
        });
    }

    evaluate_target_compatibility(request, candidate, &mut rejection_reasons);
    evaluate_runtime_compatibility(request, candidate, &mut rejection_reasons);
    evaluate_data_locality(request, candidate, &mut rejection_reasons);
    evaluate_capabilities(required_capabilities, candidate, &mut rejection_reasons);
    evaluate_dedicated_instance(request, candidate, &mut rejection_reasons);

    let accepted = rejection_reasons.is_empty();
    let reasons = if accepted {
        vec![format!(
            "candidate `{}` satisfies placement requirements",
            candidate.candidate_id
        )]
    } else {
        reason_texts(&rejection_reasons)
    };

    PlacementCandidateEvaluation {
        candidate_id: candidate.candidate_id.clone(),
        target: candidate.target,
        accepted,
        reasons,
        rejection_reasons,
    }
}

fn evaluate_target_compatibility(
    request: &PlacementRequest,
    candidate: &PlacementCandidate,
    rejection_reasons: &mut Vec<PlacementRejectionReason>,
) {
    if request.target == candidate.target {
        evaluate_execution_mode(request, candidate, rejection_reasons);
        return;
    }

    if request.target == PlacementTarget::PhysicalRobot && candidate.target.is_cloud() {
        if request.execution_mode == PlacementExecutionMode::Live {
            rejection_reasons.push(
                PlacementRejectionReason::PhysicalRequiresDeviceOrExplicitHelper {
                    candidate_id: candidate.candidate_id.clone(),
                    candidate: candidate.target,
                },
            );
            return;
        }
        evaluate_execution_mode(request, candidate, rejection_reasons);
        return;
    }

    rejection_reasons.push(PlacementRejectionReason::TargetMismatch {
        candidate_id: candidate.candidate_id.clone(),
        requested: request.target,
        candidate: candidate.target,
    });
}

fn evaluate_execution_mode(
    request: &PlacementRequest,
    candidate: &PlacementCandidate,
    rejection_reasons: &mut Vec<PlacementRejectionReason>,
) {
    if !candidate
        .supported_execution_modes
        .contains(&request.execution_mode)
    {
        rejection_reasons.push(PlacementRejectionReason::UnsupportedExecutionMode {
            candidate_id: candidate.candidate_id.clone(),
            mode: request.execution_mode,
        });
    }
}

fn evaluate_runtime_compatibility(
    request: &PlacementRequest,
    candidate: &PlacementCandidate,
    rejection_reasons: &mut Vec<PlacementRejectionReason>,
) {
    if let Some(required) = request.required_runtime_version.as_deref() {
        if candidate.runtime_version != required {
            rejection_reasons.push(PlacementRejectionReason::IncompatibleRuntime {
                candidate_id: candidate.candidate_id.clone(),
                required: required.to_string(),
                found: candidate.runtime_version.clone(),
            });
        }
    }
}

fn evaluate_data_locality(
    request: &PlacementRequest,
    candidate: &PlacementCandidate,
    rejection_reasons: &mut Vec<PlacementRejectionReason>,
) {
    if let Some(required) = request.data_locality {
        if candidate.data_locality != Some(required) {
            rejection_reasons.push(PlacementRejectionReason::DataLocalityMismatch {
                candidate_id: candidate.candidate_id.clone(),
                required,
                found: candidate.data_locality,
            });
        }
    }
}

fn evaluate_capabilities(
    required_capabilities: &[String],
    candidate: &PlacementCandidate,
    rejection_reasons: &mut Vec<PlacementRejectionReason>,
) {
    let candidate_capabilities = candidate.normalized_capabilities();
    for capability in required_capabilities {
        if !candidate_capabilities.contains(capability) {
            rejection_reasons.push(PlacementRejectionReason::MissingCapability {
                candidate_id: candidate.candidate_id.clone(),
                capability: capability.clone(),
            });
        }
    }
}

fn evaluate_dedicated_instance(
    request: &PlacementRequest,
    candidate: &PlacementCandidate,
    rejection_reasons: &mut Vec<PlacementRejectionReason>,
) {
    if request.dedicated_instance && !candidate.dedicated_instance_available {
        rejection_reasons.push(PlacementRejectionReason::DedicatedInstanceRequired {
            candidate_id: candidate.candidate_id.clone(),
        });
    }
}

fn build_decision(
    request: &PlacementRequest,
    target: PlacementTarget,
    candidate_id: Option<String>,
    status: PlacementDecisionStatus,
    reasons: Vec<String>,
    required_capabilities: Vec<String>,
    evaluated_candidates: Vec<PlacementCandidateEvaluation>,
) -> PlacementDecision {
    let selected_target = (status == PlacementDecisionStatus::Selected).then_some(target);
    let trace_audit = PlacementTraceAudit {
        schema: PLACEMENT_DECISION_SCHEMA.to_string(),
        requested_target: request.target,
        selected_target,
        selected_candidate_id: candidate_id.clone(),
        execution_mode: request.execution_mode,
        dedicated_instance: request.dedicated_instance,
        required_capabilities: required_capabilities.clone(),
        data_locality: request.data_locality,
        reasons: reasons.clone(),
    };

    PlacementDecision {
        status,
        target,
        candidate_id,
        reasons,
        dedicated_instance: request.dedicated_instance,
        required_capabilities,
        data_locality: request.data_locality,
        max_runtime_ms: request.max_runtime_ms,
        explain: PlacementExplain {
            requested_target: request.target,
            execution_mode: request.execution_mode,
            evaluated_candidates,
        },
        trace_audit,
    }
}

fn reason_texts(reasons: &[PlacementRejectionReason]) -> Vec<String> {
    reasons
        .iter()
        .map(PlacementRejectionReason::reason_text)
        .collect()
}

fn normalize_tokens(tokens: &[String]) -> Vec<String> {
    let mut normalized: Vec<_> = tokens
        .iter()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn has_blank_tokens(tokens: &[String]) -> bool {
    tokens.iter().any(|token| token.trim().is_empty())
}

fn display_locality(locality: Option<DataLocality>) -> &'static str {
    locality.map_or("unspecified", DataLocality::as_str)
}

#[cfg(test)]
#[path = "../tests/unit/placement_tests.rs"]
mod tests;
