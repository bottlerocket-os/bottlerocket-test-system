use crate::error;
use bottlerocket_types::agent_config::{
    ResultFormat, SonobuoyConfig, SonobuoyPluginConfig, SONOBUOY_RESULTS_FILENAME,
};
use log::{error, info, trace};
use model::{Outcome, TestResults};
use serde_json::Value;
use snafu::{ensure, OptionExt, ResultExt};
use std::fs::{self};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Runs the sonobuoy conformance tests according to the provided configuration and returns a test
/// result at the end.
pub async fn run_sonobuoy(
    kubeconfig_path: &str,
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
    info!("Running sonobuoy");
    let (plugin, args) = create_plugin(&sonobuoy_config.plugin)?;
    let status = Command::new("/usr/bin/sonobuoy")
        .args(kubeconfig_arg.to_owned())
        .arg("run")
        .arg("--plugin")
        .arg(plugin)
        .args(args)
        .args(k8s_image_arg)
        .status()
        .context(error::SonobuoyProcessSnafu)?;
    info!("Sonobuoy testing has started, waiting for results");

    wait_for_sonobuoy_results(kubeconfig_path).await?;

    // TODO - log something or check what happened?
    ensure!(status.success(), error::SonobuoyRunSnafu);

    results_sonobuoy(kubeconfig_path, results_dir)
}

/// Reruns the the failed tests from sonobuoy conformance that has already run in this agent.
pub async fn rerun_failed_sonobuoy(
    kubeconfig_path: &str,
    sonobuoy_config: &SonobuoyConfig,
    results_dir: &Path,
) -> Result<TestResults, error::Error> {
    let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
    let results_filepath = results_dir.join(SONOBUOY_RESULTS_FILENAME);
    trace!("Rerunning sonobuoy");
    if matches!(sonobuoy_config.plugin, SonobuoyPluginConfig::E2E(_)) {
        info!("Rerunning failed 'e2e' tests");
        let status = Command::new("/usr/bin/sonobuoy")
            .args(kubeconfig_arg.to_owned())
            .arg("e2e")
            .arg("--rerun-failed")
            .arg(results_filepath.as_os_str())
            .status()
            .context(error::SonobuoyProcessSnafu)?;
        ensure!(status.success(), error::SonobuoyRunSnafu);
        info!("Sonobuoy testing has started, waiting for results");
        wait_for_sonobuoy_results(kubeconfig_path).await?;
        results_sonobuoy(kubeconfig_path, results_dir)
    } else {
        info!("Rerunning plugin '{}'", sonobuoy_config.plugin.name());
        run_sonobuoy(kubeconfig_path, sonobuoy_config, results_dir).await
    }
    // TODO - log something or check what happened?
}

pub async fn wait_for_sonobuoy_results(kubeconfig_path: &str) -> Result<(), error::Error> {
    loop {
        let kubeconfig_arg = vec!["--kubeconfig", kubeconfig_path];
        trace!("Checking test status");
        let run_result = Command::new("/usr/bin/sonobuoy")
            .args(kubeconfig_arg)
            .arg("status")
            .arg("--json")
            .output()
            .context(error::SonobuoyProcessSnafu)?;

        let stdout = String::from_utf8_lossy(&run_result.stdout);
        info!("Parsing the following sonobuoy results output:\n{}", stdout);

        trace!("Parsing sonobuoy results as json");
        let run_status: serde_json::Value = match serde_json::from_str(&stdout) {
            Ok(run_status) => run_status,
            Err(err) => {
                error!(
                    "An error occured while serializing sonobuoy status: '{:?}' will try again. (This can happen if sonobuoy is not ready)",
                    err
                );
                tokio::time::sleep(Duration::from_secs(120)).await;
                continue;
            }
        };
        trace!("The sonobuoy results are valid json");

        let status = run_status.get("status");
        if status.is_some() && status != Some(&Value::String("running".to_string())) {
            return Ok(());
        }
        info!("Some tests are still running");

        tokio::time::sleep(Duration::from_secs(120)).await;
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

    let (passed, failed, skipped) = run_status
        .get("plugins")
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .as_array()
        .context(error::MissingSonobuoyStatusFieldSnafu { field: "plugins" })?
        .iter()
        .fold((0, 0, 0), |acc, x| {
            let (pass, fail, skip) = if let Some(result_counts) = x.get("result-counts") {
                let pass = result_counts
                    .get("passed")
                    .and_then(|count| count.as_u64())
                    .unwrap_or(0);
                let fail = result_counts
                    .get("failed")
                    .and_then(|count| count.as_u64())
                    .unwrap_or(0);
                let skip = result_counts
                    .get("skipped")
                    .and_then(|count| count.as_u64())
                    .unwrap_or(0);
                (pass, fail, skip)
            } else {
                (0, 0, 0)
            };
            (acc.0 + pass, acc.1 + fail, acc.2 + skip)
        });

    Ok(TestResults {
        outcome: if failed > 0 {
            Outcome::Fail
        } else {
            Outcome::Pass
        },
        num_passed: passed,
        num_failed: failed,
        num_skipped: skipped,
        other_info: Some(stdout.to_string()),
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
        .context(error::SonobuoyProcessSnafu)?;
    ensure!(status.success(), error::SonobuoyDeleteSnafu);

    Ok(())
}

/// Returns a string that should be used with the
/// sonobuoy command line (i.e `e2e` `customPlugin.yaml`) and any extra args that should be used.
/// If `plugin` is `Path` the string is returned.
/// If `plugin` is `EncodedYaml` a file will be created at `/tmp/<name>` containing
/// the decoded yaml.
/// If `plugin` is `CustomPlugin` a file will be created at `/tmp/<name>` using
/// `sonobuoy gen plugin` or `serde_yaml`
pub fn create_plugin(plugin: &SonobuoyPluginConfig) -> Result<(String, Vec<String>), error::Error> {
    Ok(match plugin {
        SonobuoyPluginConfig::E2E(mode) => (
            "e2e".to_string(),
            vec!["--mode".to_string(), mode.to_string()],
        ),
        SonobuoyPluginConfig::Path(path) => (path.to_string(), Vec::new()),
        SonobuoyPluginConfig::EncodedYaml { name, encoded_yaml } => {
            let path = format!("/tmp/{}.yaml", name);
            let plugin_path = Path::new(&path);
            info!("Decoding yaml for custom plugin");
            let decoded_bytes =
                base64::decode(encoded_yaml.as_bytes()).context(error::Base64DecodeSnafu {
                    what: "custom plugin",
                })?;
            info!("Storing custom plugin at {}", plugin_path.display());
            fs::write(plugin_path, decoded_bytes).context(error::WriteSnafu {
                what: "custom plugin",
            })?;
            (path, Vec::new())
        }
        SonobuoyPluginConfig::CustomPlugin {
            name,
            image,
            result_format,
        } => {
            let path = format!("/tmp/{}.yaml", name);
            let plugin_path = Path::new(&path);
            info!("Creating custom plugin");
            match result_format {
                ResultFormat::Raw => {
                    let output = Command::new("/usr/bin/sonobuoy")
                        .arg("gen")
                        .arg("plugin")
                        .args(["--name", name])
                        .args(["--image", image])
                        .output()
                        .context(error::SonobuoyProcessSnafu)?;
                    fs::write(plugin_path, &output.stdout).context(error::WriteSnafu {
                        what: "custom plugin",
                    })?;
                }
                ResultFormat::Junit => {
                    let output = Command::new("/usr/bin/sonobuoy")
                        .arg("gen")
                        .arg("plugin")
                        .args(["--name", name])
                        .args(["--image", image])
                        .args(["--format", "junit"])
                        .output()
                        .context(error::SonobuoyProcessSnafu)?;
                    info!("Storing custom plugin at {}", plugin_path.display());
                    fs::write(plugin_path, &output.stdout).context(error::WriteSnafu {
                        what: "custom plugin",
                    })?;
                }
                ResultFormat::Manual(result_files) => {
                    let yaml = serde_yaml::from_str::<serde_yaml::Mapping>(&format!(
                        r#"---
sonobuoy-config: 
  driver: Job
  plugin-name: {}
  result-format: manual
  result-files: {:?}
spec:
  command:
  - ./run.sh
  image: {}
  name: plugin
  resources: {{}}
  volumeMounts:
  - mountPath: /tmp/sonobuoy/results
    name: results"#,
                        name, result_files, image
                    ))
                    .context(error::DeserializeYamlSnafu)?;
                    let plugin =
                        serde_yaml::to_string(&yaml).context(error::DeserializeYamlSnafu)?;
                    info!("Creating plugin with yaml: \n'''\n{}\n'''", plugin);
                    info!("Storing custom plugin at {}", plugin_path.display());
                    fs::write(plugin_path, plugin).context(error::WriteSnafu {
                        what: "custom plugin",
                    })?;
                }
            }

            (path, Vec::new())
        }
    })
}
