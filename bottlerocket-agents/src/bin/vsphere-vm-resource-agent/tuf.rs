use resource_agent::provider::{IntoProviderError, ProviderResult, Resources};
use std::fs::File;
use std::path::{Path, PathBuf};
use tough::{ExpirationEnforcement, Prefix, RepositoryLoader, TargetName};
use url::Url;

const ROOT_FILE_NAME: &str = "1.root.json";

pub(crate) fn download_target(
    resources: Resources,
    metadata_url: &Url,
    targets_url: &Url,
    outdir: &Path,
    target_name: &str,
) -> ProviderResult<()> {
    // Need to download root.json. This is an unsafe operation but in the context of testing it's fine.
    let root_path = download_root(resources, metadata_url, outdir)?;
    let repository = RepositoryLoader::new(
        File::open(&root_path).context(
            resources,
            "Failed to open root.json file for loading TUF repository",
        )?,
        metadata_url.to_owned(),
        targets_url.to_owned(),
    )
    .expiration_enforcement(ExpirationEnforcement::Unsafe)
    .load()
    .context(resources, "Failed to load TUF repository")?;

    repository
        .save_target(
            &TargetName::new(target_name).context(Resources::Clear, "Unsafe target file name")?,
            outdir,
            Prefix::None,
        )
        .context(
            resources,
            format!("Failed to download target file '{}'", target_name),
        )?;

    Ok(())
}

fn download_root<P>(
    resources: Resources,
    metadata_base_url: &Url,
    outdir: P,
) -> ProviderResult<PathBuf>
where
    P: AsRef<Path>,
{
    let path = outdir.as_ref().join(ROOT_FILE_NAME);
    let url = metadata_base_url.join(ROOT_FILE_NAME).context(
        resources,
        format!(
            "Could not parse url '{}/{}'",
            metadata_base_url.as_str(),
            ROOT_FILE_NAME
        ),
    )?;

    let mut root_request = reqwest::blocking::get(url.as_str())
        .context(resources, "Could not send HTTP GET request for root.json")?
        .error_for_status()
        .context(
            Resources::Clear,
            "Bad HTTP response when downloading root.json",
        )?;

    let mut f = File::create(&path).context(resources, "Failed to create root.json file")?;
    root_request
        .copy_to(&mut f)
        .context(Resources::Clear, "Failed to copy root.json to file")?;

    Ok(path)
}
