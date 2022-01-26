use crate::error;
use crate::error::Error;
use agent_common::secrets::SecretData;
use log::info;
use snafu::{ensure, OptionExt, ResultExt};
use std::fs;
use std::path::Path;
use std::process::Command;

pub const WIREGUARD_SECRET_NAME: &str = "wireguardSecrets";
pub const WIREGUARD_CONF_PATH: &str = "/local/wg0.conf";

/// Sets up the wireguard connection with a given wireguard configuration stored as a K8s secret
pub async fn setup_wireguard(wireguard_secret: &SecretData) -> Result<(), Error> {
    let base64_encoded_wireguard_conf = String::from_utf8(
        wireguard_secret
            .get("b64-wireguard-conf")
            .context(error::WireguardConfMissingSnafu)?
            .to_owned(),
    )
    .context(error::ConversionSnafu {
        what: "wireguard_secret_name",
    })?;
    let wireguard_config_path = Path::new(WIREGUARD_CONF_PATH);
    info!("Decoding wireguard conf for setting up wireguard VPN");
    let decoded_bytes = base64::decode(base64_encoded_wireguard_conf.as_bytes()).context(
        error::Base64DecodeSnafu {
            what: "wireguard conf",
        },
    )?;
    info!(
        "Storing wireguard conf in {}",
        wireguard_config_path.display()
    );
    fs::write(wireguard_config_path, decoded_bytes).context(error::WriteSnafu {
        what: "wireguard conf",
    })?;
    let output = Command::new("/usr/bin/wg-quick")
        .env("WG_QUICK_USERSPACE_IMPLEMENTATION", "boringtun")
        .env("WG_SUDO", "1")
        .arg("up")
        .arg(WIREGUARD_CONF_PATH)
        .output()
        .context(error::ProcessSnafu { what: "wireguard" })?;
    ensure!(
        output.status.success(),
        error::WireguardRunSnafu {
            stderr: String::from_utf8_lossy(&output.stderr)
        }
    );
    Ok(())
}
