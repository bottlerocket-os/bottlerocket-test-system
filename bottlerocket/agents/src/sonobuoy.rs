use crate::error;
use bottlerocket_types::agent_config::{SonobuoyConfig, SONOBUOY_RESULTS_FILENAME};
use log::{error, info, trace};
use serde_json::Value;
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use test_agent::InfoClient;
use testsys_model::{Outcome, TestResults};

/// Runs the sonobuoy conformance tests according to the provided configuration and returns a test
/// result at the end.
pub async fn run_sonobuoy<I>(
    kubeconfig_path: &str,
    e2e_repo_config_path: Option<&str>,
    sonobuoy_config: &SonobuoyConfig,
    results_dir: &Path,
    info_client: &I,
) -> Result<TestResults, error::Error>
where
    I: InfoClient,
{
    let mut results = TestResults {
        outcome: Outcome::InProgress,
        other_info: Some("Starting test".to_string()),
        ..Default::default()
    };
    info_client
        .send_test_update(results.clone())
        .await
        .err()
        .iter()
        .for_each(|e| error!("Unable to send test update: {}", e));
    let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
    let version = sonobuoy_config
        .kubernetes_version
        .as_ref()
        .map(|version| version.full_version_with_v());
    let k8s_image_arg = match (&sonobuoy_config.kube_conformance_image, &version) {
        (Some(image), None) | (Some(image), Some(_)) => {
            vec!["--kube-conformance-image", image]
        }
        (None, Some(version)) => {
            vec!["--kubernetes-version", version]
        }
        _ => {
            vec![]
        }
    };
    let e2e_repo_arg = match e2e_repo_config_path {
        Some(e2e_repo_config_path) => {
            vec!["--e2e-repo-config", e2e_repo_config_path]
        }
        None => {
            vec![]
        }
    };
    let sonobuoy_image_arg = match &sonobuoy_config.sonobuoy_image {
        Some(sonobuoy_image_arg) => {
            vec!["--sonobuoy-image", sonobuoy_image_arg]
        }
        None => {
            vec![]
        }
    };
    info!("Running sonobuoy");
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg.to_owned())
        .arg("run")
        .arg("--plugin")
        .arg(&sonobuoy_config.plugin)
        .arg("--mode")
        .arg(&sonobuoy_config.mode.to_string())
        .args(k8s_image_arg)
        .args(e2e_repo_arg)
        .args(sonobuoy_image_arg)
        .arg("--plugin-env=e2e.E2E_EXTRA_ARGS=--non-blocking-taints=sonobuoy,node-role.kubernetes.io/control-plane,node-role.kubernetes.io/master")
        .status()
        .context(error::SonobuoyProcessSnafu)?;

    // TODO - log something or check what happened?
    ensure!(status.success(), error::SonobuoyRunSnafu);

    results.other_info = Some("Checking status".to_string());
    info_client
        .send_test_update(results)
        .await
        .err()
        .iter()
        .for_each(|e| error!("Unable to send test update: {}", e));
    info!("Sonobuoy testing has started, waiting for status to be available");
    tokio::time::timeout(
        Duration::from_secs(300),
        wait_for_sonobuoy_status(kubeconfig_path, None),
    )
    .await
    .context(error::SonobuoyTimeoutSnafu)??;
    info!("Sonobuoy status is available, waiting for test to complete");
    wait_for_sonobuoy_results(kubeconfig_path, None, info_client).await?;
    info!("Sonobuoy testing has completed, checking results");

    results_sonobuoy(kubeconfig_path, results_dir)
}

/// Reruns the the failed tests from sonobuoy conformance that has already run in this agent.
pub async fn rerun_failed_sonobuoy<I>(
    kubeconfig_path: &str,
    e2e_repo_config_path: Option<&str>,
    sonobuoy_image: Option<String>,
    results_dir: &Path,
    info_client: &I,
) -> Result<TestResults, error::Error>
where
    I: InfoClient,
{
    let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
    let results_filepath = results_dir.join(SONOBUOY_RESULTS_FILENAME);
    let e2e_repo_arg = match e2e_repo_config_path {
        Some(e2e_repo_config_path) => {
            vec!["--e2e-repo-config", e2e_repo_config_path]
        }
        None => {
            vec![]
        }
    };
    let sonobuoy_image_arg = match &sonobuoy_image {
        Some(sonobuoy_image_arg) => {
            vec!["--sonobuoy-image", sonobuoy_image_arg]
        }
        None => {
            vec![]
        }
    };
    info!("Rerunning sonobuoy");
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg.to_owned())
        .arg("run")
        .args(e2e_repo_arg)
        .args(sonobuoy_image_arg)
        .arg("--rerun-failed")
        .arg(results_filepath.as_os_str())
        .status()
        .context(error::SonobuoyProcessSnafu)?;

    // TODO - log something or check what happened?
    ensure!(status.success(), error::SonobuoyRunSnafu);

    info!("Sonobuoy testing has started, waiting for status to be available");
    tokio::time::timeout(
        Duration::from_secs(300),
        wait_for_sonobuoy_status(kubeconfig_path, None),
    )
    .await
    .context(error::SonobuoyTimeoutSnafu)??;
    info!("Sonobuoy status is available, waiting for test to complete");
    wait_for_sonobuoy_results(kubeconfig_path, None, info_client).await?;
    info!("Sonobuoy testing has completed, checking results");

    results_sonobuoy(kubeconfig_path, results_dir)
}

pub async fn wait_for_sonobuoy_status(
    kubeconfig_path: &str,
    namespace: Option<&str>,
) -> Result<(), error::Error> {
    loop {
        let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
        let namespace_arg = namespace
            .map(|namespace| vec!["--namespace", namespace])
            .unwrap_or_default();

        trace!("Checking test status");
        let run_result = Command::new("/usr/bin/sonobuoy")
            .args(kubeconfig_arg)
            .args(namespace_arg)
            .arg("status")
            .arg("--json")
            .output()
            .context(error::SonobuoyProcessSnafu)?;

        ensure!(run_result.status.success(), error::SonobuoyRunSnafu);

        let stdout = String::from_utf8_lossy(&run_result.stdout);
        info!("Parsing the following sonobuoy results output:\n{}", stdout);

        trace!("Parsing sonobuoy results as json");
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(_) => return Ok(()),
            Err(_) => {
                info!("Sonobuoy status is not ready, retrying in 30s");
                tokio::time::sleep(Duration::from_secs(30)).await
            }
        };
    }
}

pub async fn wait_for_sonobuoy_results<I>(
    kubeconfig_path: &str,
    namespace: Option<&str>,
    info_client: &I,
) -> Result<(), error::Error>
where
    I: InfoClient,
{
    let mut results = TestResults {
        outcome: Outcome::InProgress,
        other_info: Some("Running".to_string()),
        ..Default::default()
    };
    let mut retries = 0;
    loop {
        if retries > 5 {
            return Err(error::Error::SonobuoyStatus { retries });
        }
        let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
        let namespace_arg = namespace
            .map(|namespace| vec!["--namespace", namespace])
            .unwrap_or_default();

        trace!("Checking test status");
        let run_result = Command::new("/usr/bin/sonobuoy")
            .args(kubeconfig_arg)
            .args(namespace_arg)
            .arg("status")
            .arg("--json")
            .output()
            .context(error::SonobuoyProcessSnafu)?;

        ensure!(run_result.status.success(), error::SonobuoyRunSnafu);

        let stdout = String::from_utf8_lossy(&run_result.stdout);
        info!("Parsing the following sonobuoy results output:\n{}", stdout);

        trace!("Parsing sonobuoy results as json");
        let run_status: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(run_status) => run_status,
            Err(err) => {
                results.other_info = Some(format!("Status failed '{}'", retries));
                info_client
                    .send_test_update(results.clone())
                    .await
                    .err()
                    .iter()
                    .for_each(|e| error!("Unable to send test update: {}", e));

                error!(
                    "An error occured while serializing sonobuoy status: \n'{:?}'\n(This can happen if sonobuoy is not ready)\nWill try again in 30s",
                    err
                );
                retries += 1;
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }
        };
        retries = 0;
        trace!("The sonobuoy results are valid json");
        let status = run_status.get("status");
        if status.is_some()
            && status != Some(&Value::String("running".to_string()))
            && status != Some(&Value::String("post-processing".to_string()))
        {
            return Ok(());
        }
        info!("Some tests are still running");
        match process_incomplete_sonobuoy_test_results(&run_status) {
            Ok(res) => results = res,
            Err(e) => {
                error!("Unable to process sonobuoy status, {}", e);
            }
        }

        info!("Current status: {:?}", results);
        info_client
            .send_test_update(results.clone())
            .await
            .err()
            .iter()
            .for_each(|e| error!("Unable to send test update: {}", e));

        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

/// Retrieve the results from a sonobuoy test and convert them into `TestResults`.
pub fn results_sonobuoy(
    kubeconfig_path: &str,
    results_dir: &Path,
) -> Result<TestResults, error::Error> {
    let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];

    info!("Running sonobuoy retrieve");
    let results_filepath = results_dir.join(SONOBUOY_RESULTS_FILENAME);
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg.to_owned())
        .arg("retrieve")
        .arg("--filename")
        .arg(results_filepath.as_os_str())
        .status()
        .context(error::SonobuoyProcessSnafu)?;
    ensure!(status.success(), error::SonobuoyRunSnafu);

    info!("Sonobuoy testing has completed, printing results");
    let sonobuoy_results_exist_status = Command::new("/usr/bin/sonobuoy")
        .arg("results")
        .arg(results_filepath.as_os_str())
        .status()
        .context(error::SonobuoyProcessSnafu)?;

    if !sonobuoy_results_exist_status.success() {
        error!(
            "Bad exit code from 'sonobuoy results': exit {}",
            sonobuoy_results_exist_status.code().unwrap_or(1)
        )
    }

    info!("Getting Sonobuoy status");
    let run_result = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg)
        .arg("status")
        .arg("--json")
        .output()
        .context(error::SonobuoyProcessSnafu)?;

    let stdout = String::from_utf8_lossy(&run_result.stdout);
    info!("Parsing the following sonobuoy results output:\n{}", stdout);

    trace!("Parsing sonobuoy results as json");
    let run_status: serde_json::Value =
        serde_json::from_str(&stdout).context(error::DeserializeJsonSnafu)?;
    trace!("The sonobuoy results are valid json");

    process_sonobuoy_test_results(&run_status)
}

/// process_incomplete_sonobuoy_test_results parses the output from `sonobuoy status --json` output for
/// a test that has not yet completed.
pub(crate) fn process_incomplete_sonobuoy_test_results(
    run_status: &serde_json::Value,
) -> Result<TestResults, error::Error> {
    let mut num_passed: u64 = 0;
    let mut num_failed: u64 = 0;

    let plugin_results = run_status
        .get("plugins")
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .as_array()
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?;

    for result in plugin_results {
        if let Some(progress) = result.get("progress") {
            let completed: u64 = progress
                .get("completed")
                .into_iter()
                .filter_map(Value::as_u64)
                .next()
                .unwrap_or(0);

            let failed: u64 = progress
                .get("failures")
                .into_iter()
                .filter_map(Value::as_array)
                .map(Vec::len)
                .next()
                .unwrap_or(0)
                .try_into()?;

            num_passed += completed - failed;
            num_failed += failed;
        }
    }

    Ok(TestResults {
        outcome: Outcome::InProgress,
        num_passed,
        num_failed,
        num_skipped: 0,
        other_info: Some("Running".to_string()),
    })
}

/// process_sonobuoy_test_results parses the output from `sonobuoy status --json` output and gets
/// the overall status of the plugin results.
pub(crate) fn process_sonobuoy_test_results(
    run_status: &serde_json::Value,
) -> Result<TestResults, error::Error> {
    let mut num_passed: u64 = 0;
    let mut num_failed: u64 = 0;
    let mut num_skipped: u64 = 0;
    let mut progress = Vec::new();
    let mut outcome_summary = HashMap::from([
        ("pass", 0),
        ("passed", 0),
        ("fail", 0),
        ("failed", 0),
        ("timeout", 0),
        ("timed-out", 0),
    ]);

    let plugin_results = run_status
        .get("plugins")
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .as_array()
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?;

    for result in plugin_results {
        let plugin = result
            .get("plugin")
            .context(error::MissingSonobuoyStatusFieldSnafu {
                field: "plugins.[].plugin",
            })?
            .as_str()
            .context(error::MissingSonobuoyStatusFieldSnafu {
                field: "plugins.[].plugin",
            })?;

        // Sometimes a helpful log is available in the progress field, but not always.
        let progress_status = result.get("progress").map(|value| value.to_string());
        if let Some(progress_status) = progress_status {
            progress.push(format!("{}: {}", plugin, progress_status));
        }

        let result_status = result
            .get("result-status")
            .context(error::MissingSonobuoyStatusFieldSnafu {
                field: format!("plugins.{}.result-status", plugin),
            })?
            .as_str()
            .context(error::MissingSonobuoyStatusFieldSnafu {
                field: format!("plugins.{}.result-status", plugin),
            })?;
        *outcome_summary.entry(result_status).or_default() += 1;

        let result_counts =
            result
                .get("result-counts")
                .context(error::MissingSonobuoyStatusFieldSnafu {
                    field: format!("plugins.{}.result-counts", plugin),
                })?;

        let passed = result_counts
            .get("passed")
            .map(|v| v.as_u64().unwrap_or(0))
            .unwrap_or(0);
        let failed = result_counts
            .get("failed")
            .map(|v| v.as_u64().unwrap_or(0))
            .unwrap_or(0);
        let skipped = result_counts
            .get("skipped")
            .map(|v| v.as_u64().unwrap_or(0))
            .unwrap_or(0);

        num_passed += passed;
        num_failed += failed;
        num_skipped += skipped;
    }

    // Figure out what outcome to report based on what each plugin reported
    let mut outcome = Outcome::Unknown;
    if outcome_summary["pass"] > 0 || outcome_summary["passed"] > 0 {
        outcome = Outcome::Pass;
    }
    if outcome_summary["timeout"] > 0 || outcome_summary["timed-out"] > 0 {
        outcome = Outcome::Timeout;
    }
    if outcome_summary["fail"] > 0 || outcome_summary["failed"] > 0 {
        outcome = Outcome::Fail;
    }

    Ok(TestResults {
        outcome,
        num_passed,
        num_failed,
        num_skipped,
        other_info: Some(progress.join(", ")),
    })
}

/// Deletes all sonobuoy namespaces and associated resources in the target K8s cluster
pub async fn delete_sonobuoy(kubeconfig_path: &str) -> Result<(), error::Error> {
    let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
    info!("Deleting sonobuoy resources from cluster");
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg)
        .arg("delete")
        .arg("--all")
        .arg("--wait")
        .status()
        .context(error::SonobuoyProcessSnafu)?;
    ensure!(status.success(), error::SonobuoyDeleteSnafu);

    Ok(())
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

#[cfg(test)]
mod test_sonobuoy {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_process_results_pass() {
        let result =
            process_sonobuoy_test_results(
                &json!({"plugins":[{"plugin":"e2e","node":"global","status":"complete","result-status":"passed","result-counts":{"passed":6}}]})).unwrap();
        assert_eq!(result.num_passed, 6);
        assert_eq!(result.num_failed, 0);
        assert_eq!(result.num_skipped, 0);
        assert_eq!(result.outcome, Outcome::Pass);
        assert_eq!(result.other_info.unwrap(), "");
    }

    #[test]
    fn test_process_results_failed() {
        let result =
            process_sonobuoy_test_results(
                &json!({"plugins":[{"plugin":"e2e","node":"global","status":"complete","result-status":"failed","result-counts":{"failed":1}}]})).unwrap();
        assert_eq!(result.num_passed, 0);
        assert_eq!(result.num_failed, 1);
        assert_eq!(result.num_skipped, 0);
        assert_eq!(result.outcome, Outcome::Fail);
        assert_eq!(result.other_info.unwrap(), "");
    }

    #[test]
    fn test_process_results_timeout() {
        let result =
            process_sonobuoy_test_results(
                &json!({"plugins":[{"plugin":"e2e","node":"global","status":"complete","result-status":"timed-out","result-counts":{"failed":1}}]})).unwrap();
        assert_eq!(result.num_passed, 0);
        assert_eq!(result.num_failed, 1);
        assert_eq!(result.num_skipped, 0);
        assert_eq!(result.outcome, Outcome::Timeout);
        assert_eq!(result.other_info.unwrap(), "");
    }

    #[test]
    fn test_process_results_progress_object() {
        let result =
            process_sonobuoy_test_results(
                &json!({"plugins":[{"plugin":"e2e","progress":{"name":"e2e","node":"global","timestamp":"2022-12-08T15:37:23.007805243Z","msg":"Test Suite completed","total":1,"completed":1},"status":"complete","result-status":"timed-out","result-counts":{"failed":1}}]})).unwrap();
        assert_eq!(result.num_passed, 0);
        assert_eq!(result.num_failed, 1);
        assert_eq!(result.num_skipped, 0);
        assert_eq!(result.outcome, Outcome::Timeout);
        assert_eq!(result.other_info.unwrap(), "e2e: {\"name\":\"e2e\",\"node\":\"global\",\"timestamp\":\"2022-12-08T15:37:23.007805243Z\",\"msg\":\"Test Suite completed\",\"total\":1,\"completed\":1}");
    }

    #[test]
    fn test_process_results_multiple_pass() {
        // All must pass to report passing status.
        let result =
            process_sonobuoy_test_results(
                &json!({
                    "plugins":[
                        {"plugin":"smoketest","node":"global","status":"complete","result-status":"pass","result-counts":{"passed":1}},
                        {"plugin":"workload","node":"global","status":"complete","result-status":"pass","result-counts":{"passed":1,"skipped":1}},
                    ]})
                ).unwrap();
        assert_eq!(result.num_passed, 2);
        assert_eq!(result.num_failed, 0);
        assert_eq!(result.num_skipped, 1);
        assert_eq!(result.outcome, Outcome::Pass);
        assert_eq!(result.other_info.unwrap(), "");
    }

    #[test]
    fn test_process_results_multiple_pass_and_fail() {
        // Verify that is one fails, overall status is reported as failure.
        let result =
            process_sonobuoy_test_results(
                &json!({
                    "plugins":[
                        {"plugin":"smoketest","node":"global","status":"complete","result-status":"pass","result-counts":{"passed":1}},
                        {"plugin":"workload","node":"global","status":"complete","result-status":"fail","result-counts":{"failed":1,"skipped":1}},
                    ]})
                ).unwrap();
        assert_eq!(result.num_passed, 1);
        assert_eq!(result.num_failed, 1);
        assert_eq!(result.num_skipped, 1);
        assert_eq!(result.outcome, Outcome::Fail);
        assert_eq!(result.other_info.unwrap(), "");
    }

    #[test]
    fn test_process_results_multiple_pass_and_timeout() {
        // Timeout takes precedence over passing.
        let result =
            process_sonobuoy_test_results(
                &json!({
                    "plugins":[
                        {"plugin":"smoketest","node":"global","status":"complete","result-status":"pass","result-counts":{"passed":1}},
                        {"plugin":"workload","node":"global","status":"complete","result-status":"timeout","result-counts":{"failed":1,"skipped":1}},
                    ]})
                ).unwrap();
        assert_eq!(result.num_passed, 1);
        assert_eq!(result.num_failed, 1);
        assert_eq!(result.num_skipped, 1);
        assert_eq!(result.outcome, Outcome::Timeout);
        assert_eq!(result.other_info.unwrap(), "");
    }

    #[test]
    fn test_process_results_multiple_timeout_and_failure() {
        // Failure takes precendence over timeout.
        let result =
            process_sonobuoy_test_results(
                &json!({
                    "plugins":[
                        {"plugin":"smoketest","node":"global","status":"complete","result-status":"failed","result-counts":{"failed":1}},
                        {"plugin":"workload","node":"global","status":"complete","result-status":"timeout","result-counts":{"skipped":1}},
                    ]})
                ).unwrap();
        assert_eq!(result.num_passed, 0);
        assert_eq!(result.num_failed, 1);
        assert_eq!(result.num_skipped, 1);
        assert_eq!(result.outcome, Outcome::Fail);
        assert_eq!(result.other_info.unwrap(), "");
    }

    #[test]
    fn test_process_results_other_info() {
        // All must pass to report passing status.
        let result =
            process_sonobuoy_test_results(
                &json!({
                    "plugins":[
                        {"plugin":"smoketest","progress":"one","status":"complete","result-status":"pass","result-counts":{"passed":1}},
                        {"plugin":"workload","progress":"two","status":"complete","result-status":"pass","result-counts":{"passed":1,"skipped":1}},
                    ]})
                ).unwrap();
        assert_eq!(result.num_passed, 2);
        assert_eq!(result.num_failed, 0);
        assert_eq!(result.num_skipped, 1);
        assert_eq!(result.outcome, Outcome::Pass);
        assert_eq!(
            result.other_info.unwrap(),
            "smoketest: \"one\", workload: \"two\""
        );
    }
}
