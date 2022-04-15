use crate::error::{self, Result};
use bottlerocket_types::agent_config::{ResultFormat, SonobuoyMode};
use snafu::ResultExt;
use std::{fs::read_to_string, path::PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt, Clone)]
pub(crate) struct SonobuoyPluginConfig {
    /// The name for a custom sonobuoy plugin or an existing sonobuoy plugin i.e `e2e`
    #[structopt(long = "plugin-name")]
    plugin_name: Option<String>,
    /// The mode used for the sonobuoy test. One of `non-disruptive-conformance`,
    /// `certified-conformance`, `quick`. Although the Sonobuoy binary defaults to
    /// `non-disruptive-conformance`, we default to `quick` to make a quick test the most ergonomic.
    #[structopt(long = "sonobuoy-mode")]
    sonobuoy_mode: Option<SonobuoyMode>,
    /// The path to an existing yaml file for a custom sonobuoy plugin
    #[structopt(long = "plugin-file", parse(from_os_str), conflicts_with_all=&["plugin-image", "plugin-result-format", "result-file", "plugin-encoded-yaml"])]
    plugin_file_path: Option<PathBuf>,
    /// The encoded yaml for a sonobuoy plugin (must also include `plugin-name`)
    #[structopt(long="plugin-encoded-yaml", requires="plugin-name", conflicts_with_all=&["plugin-file", "plugin-image", "plugin-result-format", "result-file"])]
    plugin_encoded_yaml: Option<String>,
    /// The image for a sonobuoy plugin (must also include `plugin-name` and `plugin-result-format`)
    #[structopt(long="plugin-image", requires_all=&["plugin-name","plugin-result-format"], conflicts_with_all=&["plugin_file", "encoded-yaml"])]
    plugin_image: Option<String>,
    /// The result-format for a sonobuoy plugin the default value is raw (must also include `plugin-name` and `plugin-image`)
    #[structopt(long="plugin-result-format", requires_all=&["plugin-name","plugin-image"], conflicts_with_all=&["plugin_file", "encoded-yaml"], possible_values=&["raw", "junit", "manual"])]
    plugin_result_format: Option<String>,
    /// The result-files for a sonobuoy plugin (must also include `plugin-name` and `plugin-image`)
    #[structopt(long = "plugin-result-file", conflicts_with_all=&["plugin_file", "encoded-yaml"])]
    plugin_result_files: Vec<String>,
}

pub(crate) fn create_plugin_config(
    config: &SonobuoyPluginConfig,
) -> Result<bottlerocket_types::agent_config::SonobuoyPluginConfig> {
    Ok(
        match (
            config.plugin_name.as_deref(),
            &config.plugin_encoded_yaml,
            &config.plugin_image,
            &config.plugin_file_path,
            &config.sonobuoy_mode,
        ) {
            // The default case
            (None, None, None, None, mode) | (Some("e2e"), None, None, None, mode) => {
                bottlerocket_types::agent_config::SonobuoyPluginConfig::E2E(
                    mode.unwrap_or(SonobuoyMode::Quick),
                )
            }
            (Some(name), None, None, None, None) => {
                bottlerocket_types::agent_config::SonobuoyPluginConfig::Path(name.to_string())
            }
            (Some(name), Some(encoded_yaml), None, None, None) => {
                bottlerocket_types::agent_config::SonobuoyPluginConfig::EncodedYaml {
                    name: name.to_string(),
                    encoded_yaml: encoded_yaml.to_string(),
                }
            }
            (Some(name), None, Some(image), None, None) => {
                bottlerocket_types::agent_config::SonobuoyPluginConfig::CustomPlugin {
                    name: name.to_string(),
                    image: image.to_string(),
                    result_format: match config.plugin_result_format.as_deref() {
                        Some("junit") => ResultFormat::Junit,
                        Some("manual") => ResultFormat::Manual(config.plugin_result_files.clone()),
                        _ => ResultFormat::Raw,
                    },
                }
            }
            (Some(name), None, None, Some(path), None) => {
                let encoded_yaml =
                    base64::encode(read_to_string(path).context(error::FileSnafu { path })?);
                bottlerocket_types::agent_config::SonobuoyPluginConfig::EncodedYaml {
                    name: name.to_string(),
                    encoded_yaml,
                }
            }
            (_, _, _, _, _) => {
                return Err(error::Error::InvalidArguments {
                    why: "The arguments provided for the custom sonobuoy \
                        config were not a recognized configuration."
                        .to_string(),
                })
            }
        },
    )
}
