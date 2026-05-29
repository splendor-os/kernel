mod kernel_e2e_0_03;

#[tokio::test]
async fn kernel_e2e_through_0_03_scenarios_write_report() -> kernel_e2e_0_03::TestResult<()> {
    kernel_e2e_0_03::write_aggregate_report_from_env().await?;
    Ok(())
}
