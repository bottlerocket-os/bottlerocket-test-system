use model::SecretName;
use resource_agent::clients::InfoClient;
use resource_agent::provider::{IntoProviderError, ProviderResult, Resources};
use std::env;

// Helper for getting the vsphere credentials and setting up GOVC_USERNAME and GOVC_PASSWORD env vars
pub async fn vsphere_credentials<I>(
    client: &I,
    vsphere_secret_name: &SecretName,
    resource: &Resources,
) -> ProviderResult<()>
where
    I: InfoClient,
{
    let vsphere_secret = client.get_secret(vsphere_secret_name).await.context(
        Resources::Clear,
        format!("Error getting secret '{}'", vsphere_secret_name),
    )?;

    let username = String::from_utf8(
        vsphere_secret
            .get("username")
            .context(
                resource,
                format!(
                    "vsphere username missing from secret '{}'",
                    vsphere_secret_name
                ),
            )?
            .to_owned(),
    )
    .context(resource, "Could not convert vsphere username to String")?;
    let password = String::from_utf8(
        vsphere_secret
            .get("password")
            .context(
                resource,
                format!(
                    "vsphere password missing from secret '{}'",
                    vsphere_secret_name
                ),
            )?
            .to_owned(),
    )
    .context(resource, "Could not convert secret-access-key to String")?;
    env::set_var("GOVC_USERNAME", &username);
    env::set_var("GOVC_PASSWORD", &password);
    env::set_var("EKSA_VSPHERE_USERNAME", username);
    env::set_var("EKSA_VSPHERE_PASSWORD", password);
    Ok(())
}
