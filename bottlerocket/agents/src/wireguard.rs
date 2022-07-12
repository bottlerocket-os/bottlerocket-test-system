use crate::error::Error;
use crate::{base64_decode_write_file, error};
use agent_common::secrets::SecretData;
use log::{debug, info};
use snafu::{ensure, OptionExt, ResultExt};
use std::process::Command;

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
    debug!("Decoding wireguard conf for setting up wireguard VPN");
    base64_decode_write_file(&base64_encoded_wireguard_conf, WIREGUARD_CONF_PATH).await?;
    info!("Stored wireguard conf in {}", WIREGUARD_CONF_PATH);
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
