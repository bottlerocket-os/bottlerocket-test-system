use crate::error;
use bottlerocket_types::agent_config::{SonobuoyConfig, SONOBUOY_RESULTS_FILENAME};
use log::{error, info, trace};
use model::{Outcome, TestResults};
use snafu::{ensure, OptionExt, ResultExt};
use std::path::Path;
use std::process::Command;

/// Runs the sonobuoy conformance tests according to the provided configuration and returns a test
/// result at the end.
pub async fn run_sonobuoy(
    kubeconfig_path: &str,
    e2e_repo_config_path: Option<&str>,
    sonobuoy_config: &SonobuoyConfig,
    results_dir: &Path,
) -> Result<TestResults, error::Error> {
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
    info!("Running sonobuoy");
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg.to_owned())
        .arg("run")
        .arg("--wait")
        .arg("--plugin")
        .arg(&sonobuoy_config.plugin)
        .arg("--mode")
        .arg(&sonobuoy_config.mode.to_string())
        .args(k8s_image_arg)
        .args(e2e_repo_arg)
        .status()
        .context(error::SonobuoyProcessSnafu)?;
    info!("Sonobuoy testing has completed, checking results");

    // TODO - log something or check what happened?
    ensure!(status.success(), error::SonobuoyRunSnafu);

    results_sonobuoy(kubeconfig_path, sonobuoy_config, results_dir)
}

/// Reruns the the failed tests from sonobuoy conformance that has already run in this agent.
pub async fn rerun_failed_sonobuoy(
    kubeconfig_path: &str,
    e2e_repo_config_path: Option<&str>,
    sonobuoy_config: &SonobuoyConfig,
    results_dir: &Path,
) -> Result<TestResults, error::Error> {
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
    info!("Rerunning sonobuoy");
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg.to_owned())
        .arg("run")
        .args(e2e_repo_arg)
        .arg("--wait")
        .arg("--rerun-failed")
        .arg(results_filepath.as_os_str())
        .status()
        .context(error::SonobuoyProcessSnafu)?;
    info!("Sonobuoy testing has completed, checking results");

    // TODO - log something or check what happened?
    ensure!(status.success(), error::SonobuoyRunSnafu);

    results_sonobuoy(kubeconfig_path, sonobuoy_config, results_dir)
}

/// Retrieve the results from a sonobuoy test and convert them into `TestResults`.
pub fn results_sonobuoy(
    kubeconfig_path: &str,
    sonobuoy_config: &SonobuoyConfig,
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

    let e2e_status = run_status
        .get("plugins")
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .as_array()
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .first()
        .context(error::MissingSonobuoyStatusFieldSnafu {
            field: format!("plugins.{}", sonobuoy_config.plugin),
        })?;

    // Sometimes a helpful log is available in the progress field, but not always.
    let progress_status = e2e_status
        .get("progress")
        .map(|value| value.to_string())
        .unwrap_or_else(|| "".to_string());

    let result_status = e2e_status
        .get("result-status")
        .context(error::MissingSonobuoyStatusFieldSnafu {
            field: format!("plugins.{}.result-status", sonobuoy_config.plugin),
        })?
        .as_str()
        .context(error::MissingSonobuoyStatusFieldSnafu {
            field: format!("plugins.{}.result-status", sonobuoy_config.plugin),
        })?;

    let result_counts = run_status
        .get("plugins")
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .as_array()
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .first()
        .context(error::MissingSonobuoyStatusFieldSnafu {
            field: format!("plugins.{}", sonobuoy_config.plugin),
        })?
        .get("result-counts")
        .context(error::MissingSonobuoyStatusFieldSnafu {
            field: format!("plugins.{}.result-counts", sonobuoy_config.plugin),
        })?;

    let num_passed = result_counts
        .get("passed")
        .map(|v| v.as_u64().unwrap_or(0))
        .unwrap_or(0);

    let num_failed = result_counts
        .get("failed")
        .map(|v| v.as_u64().unwrap_or(0))
        .unwrap_or(0);

    let num_skipped = result_counts
        .get("skipped")
        .map(|v| v.as_u64().unwrap_or(0))
        .unwrap_or(0);

    Ok(TestResults {
        outcome: match result_status {
            "pass" | "passed" => Outcome::Pass,
            "fail" | "failed" => Outcome::Fail,
            "timeout" | "timed-out" => Outcome::Timeout,
            _ => Outcome::Unknown,
        },
        num_passed,
        num_failed,
        num_skipped,
        other_info: Some(progress_status),
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
