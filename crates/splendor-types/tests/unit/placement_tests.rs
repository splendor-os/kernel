use super::*;

const RUNTIME_VERSION: &str = "splendor-0.03-dev";

fn candidate(
    candidate_id: &str,
    target: PlacementTarget,
    capabilities: &[&str],
) -> PlacementCandidate {
    PlacementCandidate {
        candidate_id: candidate_id.to_string(),
        target,
        capabilities: capabilities
            .iter()
            .map(|capability| capability.to_string())
            .collect(),
        data_locality: None,
        runtime_version: RUNTIME_VERSION.to_string(),
        dedicated_instance_available: false,
        available: true,
        supported_execution_modes: vec![PlacementExecutionMode::Live],
    }
}

fn request(target: PlacementTarget, capabilities: &[&str]) -> PlacementRequest {
    PlacementRequest {
        target,
        required_capabilities: capabilities
            .iter()
            .map(|capability| capability.to_string())
            .collect(),
        data_locality: None,
        dedicated_instance: false,
        required_runtime_version: Some(RUNTIME_VERSION.to_string()),
        max_runtime_ms: None,
        execution_mode: PlacementExecutionMode::Live,
    }
}

#[test]
fn selects_matching_cloud_target_deterministically() {
    let mut request = request(
        PlacementTarget::ResidentCloudPool,
        &[
            "http.egress.restricted",
            "local.llm.small",
            "http.egress.restricted",
        ],
    );
    request.data_locality = Some(DataLocality::Cloud);

    let mut later = candidate(
        "resident-z",
        PlacementTarget::ResidentCloudPool,
        &["local.llm.small", "http.egress.restricted"],
    );
    later.data_locality = Some(DataLocality::Cloud);
    let mut earlier = candidate(
        "resident-a",
        PlacementTarget::ResidentCloudPool,
        &["http.egress.restricted", "local.llm.small"],
    );
    earlier.data_locality = Some(DataLocality::Cloud);

    let decision = select_placement(&request, &[later, earlier]);

    assert_eq!(decision.status, PlacementDecisionStatus::Selected);
    assert_eq!(decision.target, PlacementTarget::ResidentCloudPool);
    assert_eq!(decision.candidate_id.as_deref(), Some("resident-a"));
    assert_eq!(
        decision.required_capabilities,
        vec![
            "http.egress.restricted".to_string(),
            "local.llm.small".to_string()
        ]
    );
    assert_eq!(decision.explain.evaluated_candidates.len(), 1);
    assert!(decision.explain.evaluated_candidates[0].accepted);
}

#[test]
fn covers_cloud_vpc_on_prem_edge_physical_and_desktop_targets() {
    let cases = [
        (PlacementTarget::EphemeralCloud, Some(DataLocality::Cloud)),
        (
            PlacementTarget::ResidentCloudPool,
            Some(DataLocality::Cloud),
        ),
        (PlacementTarget::CustomerVpc, Some(DataLocality::Vpc)),
        (PlacementTarget::OnPrem, Some(DataLocality::OnPrem)),
        (PlacementTarget::EdgeDevice, Some(DataLocality::Device)),
        (PlacementTarget::PhysicalRobot, Some(DataLocality::Device)),
        (PlacementTarget::DesktopSidecar, None),
    ];

    for (target, locality) in cases {
        let mut request = request(target, &["runtime.basic"]);
        request.data_locality = locality;
        let mut candidate = candidate(
            &format!("candidate-{}", target.as_str()),
            target,
            &["runtime.basic"],
        );
        candidate.data_locality = locality;

        let decision = select_placement(&request, &[candidate]);

        assert_eq!(
            decision.status,
            PlacementDecisionStatus::Selected,
            "{target:?}"
        );
        assert_eq!(decision.target, target);
        assert_eq!(decision.data_locality, locality);
        assert_eq!(decision.trace_audit.data_locality, locality);
    }
}

#[test]
fn preserves_data_locality_in_decision_and_trace_audit_output() {
    let mut request = request(PlacementTarget::CustomerVpc, &["sql.read"]);
    request.data_locality = Some(DataLocality::Vpc);
    request.max_runtime_ms = Some(30_000);

    let mut candidate = candidate("vpc-1", PlacementTarget::CustomerVpc, &["sql.read"]);
    candidate.data_locality = Some(DataLocality::Vpc);

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Selected);
    assert_eq!(decision.data_locality, Some(DataLocality::Vpc));
    assert_eq!(decision.max_runtime_ms, Some(30_000));
    assert_eq!(decision.trace_audit.schema, PLACEMENT_DECISION_SCHEMA);
    assert_eq!(decision.trace_audit.data_locality, Some(DataLocality::Vpc));
    assert_eq!(
        decision.trace_audit.selected_candidate_id.as_deref(),
        Some("vpc-1")
    );
    assert!(decision
        .trace_audit
        .reasons
        .iter()
        .any(|reason| reason.contains("preserved data locality")));
}

#[test]
fn rejects_when_required_capability_is_unavailable() {
    let request = request(PlacementTarget::OnPrem, &["sql.read", "artifact.create"]);
    let candidate = candidate("onprem-1", PlacementTarget::OnPrem, &["sql.read"]);

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert_eq!(decision.candidate_id, None);
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("missing capability `artifact.create`")));
    assert!(matches!(
        &decision.explain.evaluated_candidates[0].rejection_reasons[0],
        PlacementRejectionReason::MissingCapability { capability, .. }
            if capability == "artifact.create"
    ));
}

#[test]
fn rejects_when_no_matching_node_is_available() {
    let request = request(PlacementTarget::EdgeDevice, &["camera.rgb"]);

    let decision = select_placement(&request, &[]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason == "no placement candidates were supplied"));
    assert!(decision.explain.evaluated_candidates.is_empty());
    assert_eq!(decision.trace_audit.selected_candidate_id, None);
}

#[test]
fn rejects_when_supplied_nodes_do_not_match_target_class() {
    let request = request(PlacementTarget::DesktopSidecar, &["file.read"]);
    let candidate = candidate(
        "cloud-only",
        PlacementTarget::EphemeralCloud,
        &["file.read"],
    );

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("does not match requested target `desktop_sidecar`")));
    assert!(matches!(
        &decision.explain.evaluated_candidates[0].rejection_reasons[0],
        PlacementRejectionReason::TargetMismatch { requested, candidate, .. }
            if *requested == PlacementTarget::DesktopSidecar
                && *candidate == PlacementTarget::EphemeralCloud
    ));
}

#[test]
fn rejects_unavailable_candidate_without_silent_fallback() {
    let request = request(PlacementTarget::ResidentCloudPool, &["runtime.basic"]);
    let mut candidate = candidate(
        "resident-offline",
        PlacementTarget::ResidentCloudPool,
        &["runtime.basic"],
    );
    candidate.available = false;

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(matches!(
        &decision.explain.evaluated_candidates[0].rejection_reasons[0],
        PlacementRejectionReason::CandidateUnavailable { candidate_id }
            if candidate_id == "resident-offline"
    ));
}

#[test]
fn rejects_invalid_candidate_metadata_fail_closed() {
    let request = request(PlacementTarget::EphemeralCloud, &["runtime.basic"]);
    let mut candidate = candidate("", PlacementTarget::EphemeralCloud, &["runtime.basic", ""]);
    candidate.runtime_version = " ".to_string();
    candidate.supported_execution_modes = Vec::new();

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(decision.explain.evaluated_candidates[0]
        .rejection_reasons
        .iter()
        .any(|reason| matches!(
            reason,
            PlacementRejectionReason::InvalidCandidate { reason, .. }
                if reason == "candidate_id is required"
        )));
    assert!(decision.explain.evaluated_candidates[0]
        .rejection_reasons
        .iter()
        .any(|reason| matches!(
            reason,
            PlacementRejectionReason::InvalidCandidate { reason, .. }
                if reason == "candidate runtime version is required"
        )));
    assert!(decision.explain.evaluated_candidates[0]
        .rejection_reasons
        .iter()
        .any(|reason| matches!(
            reason,
            PlacementRejectionReason::InvalidCandidate { reason, .. }
                if reason == "candidate must declare at least one execution mode"
        )));
}

#[test]
fn rejects_incompatible_runtime_version() {
    let mut request = request(PlacementTarget::EphemeralCloud, &["python.policy"]);
    request.required_runtime_version = Some("splendor-0.03-dev".to_string());
    let mut candidate = candidate(
        "cloud-old-runtime",
        PlacementTarget::EphemeralCloud,
        &["python.policy"],
    );
    candidate.runtime_version = "splendor-0.02-dev".to_string();

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(matches!(
        &decision.explain.evaluated_candidates[0].rejection_reasons[0],
        PlacementRejectionReason::IncompatibleRuntime { required, found, .. }
            if required == "splendor-0.03-dev" && found == "splendor-0.02-dev"
    ));
}

#[test]
fn rejects_dedicated_instance_requirement_when_unavailable() {
    let mut request = request(PlacementTarget::CustomerVpc, &["finance.read"]);
    request.data_locality = Some(DataLocality::Vpc);
    request.dedicated_instance = true;
    let mut candidate = candidate(
        "shared-vpc",
        PlacementTarget::CustomerVpc,
        &["finance.read"],
    );
    candidate.data_locality = Some(DataLocality::Vpc);
    candidate.dedicated_instance_available = false;

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(decision.dedicated_instance);
    assert!(decision
        .reasons
        .iter()
        .any(|reason| reason.contains("dedicated instance")));
    assert!(matches!(
        &decision.explain.evaluated_candidates[0].rejection_reasons[0],
        PlacementRejectionReason::DedicatedInstanceRequired { candidate_id }
            if candidate_id == "shared-vpc"
    ));
}

#[test]
fn does_not_place_live_physical_request_on_generic_cloud_without_helper() {
    let mut request = request(PlacementTarget::PhysicalRobot, &["motion.waypoint"]);
    request.data_locality = Some(DataLocality::Device);
    request.execution_mode = PlacementExecutionMode::Live;

    let mut cloud = candidate(
        "generic-cloud",
        PlacementTarget::EphemeralCloud,
        &["motion.waypoint"],
    );
    cloud.data_locality = Some(DataLocality::Device);

    let decision = select_placement(&request, &[cloud]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(decision.reasons.iter().any(|reason| {
        reason.contains(
            "cannot run a live physical request without explicit simulation or cloud_helper mode",
        )
    }));
    assert!(matches!(
        &decision.explain.evaluated_candidates[0].rejection_reasons[0],
        PlacementRejectionReason::PhysicalRequiresDeviceOrExplicitHelper { .. }
    ));
}

#[test]
fn allows_physical_cloud_helper_only_when_explicit() {
    let mut request = request(PlacementTarget::PhysicalRobot, &["route.optimize"]);
    request.execution_mode = PlacementExecutionMode::CloudHelper;
    request.data_locality = Some(DataLocality::Cloud);

    let mut helper = candidate(
        "route-helper",
        PlacementTarget::ResidentCloudPool,
        &["route.optimize"],
    );
    helper.data_locality = Some(DataLocality::Cloud);
    helper.supported_execution_modes = vec![PlacementExecutionMode::CloudHelper];

    let decision = select_placement(&request, &[helper]);

    assert_eq!(decision.status, PlacementDecisionStatus::Selected);
    assert_eq!(decision.target, PlacementTarget::ResidentCloudPool);
    assert_eq!(decision.candidate_id.as_deref(), Some("route-helper"));
    assert!(decision
        .reasons
        .iter()
        .any(|reason| { reason.contains("no live actuator authority granted to cloud target") }));
}

#[test]
fn invalid_capability_tokens_fail_closed_without_candidate_evaluation() {
    let request = PlacementRequest {
        target: PlacementTarget::DesktopSidecar,
        required_capabilities: vec!["file.read".to_string(), " ".to_string()],
        data_locality: None,
        dedicated_instance: false,
        required_runtime_version: Some(RUNTIME_VERSION.to_string()),
        max_runtime_ms: None,
        execution_mode: PlacementExecutionMode::Live,
    };
    let candidate = candidate("desktop", PlacementTarget::DesktopSidecar, &["file.read"]);

    let decision = select_placement(&request, &[candidate]);

    assert_eq!(decision.status, PlacementDecisionStatus::Rejected);
    assert!(decision.explain.evaluated_candidates.is_empty());
    assert!(decision
        .reasons
        .iter()
        .any(|reason| { reason.contains("required capabilities must not contain blank tokens") }));
}

#[test]
fn placement_decision_serializes_without_permission_or_action_expansion() {
    let mut request = request(PlacementTarget::OnPrem, &["sql.read"]);
    request.data_locality = Some(DataLocality::OnPrem);
    let mut candidate = candidate("onprem-1", PlacementTarget::OnPrem, &["sql.read"]);
    candidate.data_locality = Some(DataLocality::OnPrem);

    let decision = select_placement(&request, &[candidate]);
    let payload = serde_json::to_vec(&decision).expect("serialize placement decision");
    let decoded: PlacementDecision =
        serde_json::from_slice(&payload).expect("deserialize decision");

    assert_eq!(decoded, decision);

    let json = serde_json::to_string(&decision).expect("serialize placement decision");
    for forbidden in [
        "allowed_permissions",
        "required_permissions",
        "allowed_actions",
        "allowed_adapters",
    ] {
        assert!(
            !json.contains(forbidden),
            "placement decision must not add authority field `{forbidden}`"
        );
    }
}
