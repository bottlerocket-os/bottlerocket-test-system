use crate::{error, SonobuoyConfig};
use log::info;
use model::{Outcome, TestResults};
use snafu::{ensure, OptionExt, ResultExt};
use std::path::PathBuf;
use std::process::Command;

/// Runs the sonobuoy conformance tests according to the provided configuration and returns a test
/// result at the end.
pub async fn run_sonobuoy(
    kubeconfig_path: &str,
    sonobuoy_config: &SonobuoyConfig,
    results_dir: &PathBuf,
) -> Result<TestResults, error::Error> {
    let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
    let k8s_image_arg = match (
        &sonobuoy_config.kube_conformance_image,
        &sonobuoy_config.kubernetes_version,
    ) {
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
    info!("Running sonobuoy");
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg.to_owned())
        .arg("run")
        .arg("--wait")
        .arg("--plugin")
        .arg(&sonobuoy_config.plugin)
        .arg("--mode")
        .arg(&sonobuoy_config.mode)
        .args(k8s_image_arg)
        .status()
        .context(error::SonobuoyProcess)?;
    ensure!(status.success(), error::SonobuoyRun);

    let status = Command::new("/usr/bin/sonobuoy")
        .current_dir(results_dir.to_str().context(error::ResultsLocation)?)
        .args(kubeconfig_arg.to_owned())
        .arg("retrieve")
        .status()
        .context(error::SonobuoyProcess)?;
    ensure!(status.success(), error::SonobuoyRun);

    let run_result = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg)
        .arg("status")
        .arg("--json")
        .output()
        .context(error::SonobuoyProcess)?;

    let run_status: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&run_result.stdout))
            .context(error::DeserializeJson)?;
    let e2e_status = run_status
        .get("plugins")
        .context(error::MissingSonobuoyStatusField { field: "plugins" })?
        .as_array()
        .context(error::MissingSonobuoyStatusField { field: "plugins" })?
        .first()
        .context(error::MissingSonobuoyStatusField {
            field: format!("plugins.{}", sonobuoy_config.plugin),
        })?;
    let progress_status =
        e2e_status
            .get("progress")
            .context(error::MissingSonobuoyStatusField {
                field: format!("plugins.{}.progress", sonobuoy_config.plugin),
            })?;
    let result_status = e2e_status
        .get("result-status")
        .context(error::MissingSonobuoyStatusField {
            field: format!("plugins.{}.result-status", sonobuoy_config.plugin),
        })?
        .as_str()
        .context(error::MissingSonobuoyStatusField {
            field: format!("plugins.{}.result-status", sonobuoy_config.plugin),
        })?;
    let result_counts = run_status
        .get("plugins")
        .context(error::MissingSonobuoyStatusField { field: "plugins" })?
        .as_array()
        .context(error::MissingSonobuoyStatusField { field: "plugins" })?
        .first()
        .context(error::MissingSonobuoyStatusField {
            field: format!("plugins.{}", sonobuoy_config.plugin),
        })?
        .get("result-counts")
        .context(error::MissingSonobuoyStatusField {
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
        other_info: Some(progress_status.to_owned().to_string()),
    })
}

/// Deletes all sonobuoy namespaces and associated resources in the target K8s cluster
pub async fn delete_sonobuoy(kubeconfig_path: &str) -> Result<(), error::Error> {
    let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
    info!("Deleting sonobuoy resources from cluster");
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg)
        .arg("delete")
        .arg("--wait")
        .status()
        .context(error::SonobuoyProcess)?;
    ensure!(status.success(), error::SonobuoyDelete);

    Ok(())
}
