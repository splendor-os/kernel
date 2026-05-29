mod kernel_e2e_0_03;

#[tokio::test]
async fn k_e2e_003_local_multi_agent_evidence_row() -> kernel_e2e_0_03::TestResult<()> {
    kernel_e2e_0_03::run_single_scenario_check("K-E2E-003").await
}
