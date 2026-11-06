use super::input::{unique_ids, Author, BackendReference, BuildKind, Links};
use super::{safe_component, Asset, BuildOutput, Copy, PackageConfig, PackageSources};
use serde::Serialize;
use typed_path::Utf8WindowsPath;

pub(super) struct PackagePlan {
    pub(super) manifest: Manifest,
    pub(super) sources: PackageSources,
    pub(super) assets: Vec<Asset>,
}

#[derive(Serialize)]
pub(super) struct Manifest {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<Author>,
    links: Links,
    plugins: Vec<ManifestPlugin>,
}

#[derive(Serialize)]
struct ManifestPlugin {
    id: String,
    kind: &'static str,
    name: String,
    entrypoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<Option<BackendReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<String>,
}

fn copy(source: String, id: &str, sources: &mut PackageSources) -> Result<String, String> {
    // Windows parsing recognizes both separators on every native host.
    let basename = Utf8WindowsPath::new(&source).file_name().unwrap_or("");
    if !safe_component(basename) || !source.ends_with(basename) {
        return Err(format!("Invalid build output filename '{source}'"));
    }
    let destination = format!("{id}/{basename}");
    sources.add(Copy {
        source,
        destination: destination.clone(),
    })?;
    Ok(destination)
}

fn plan_output(
    output: BuildOutput,
    sources: &mut PackageSources,
    assets: &mut Vec<Asset>,
) -> Result<ManifestPlugin, String> {
    let entrypoint = copy(output.file_name, &output.id, sources)?;
    let (kind, css, backend) = match output.kind {
        BuildKind::Frontend { css, backend } => ("frontend", css, backend),
        BuildKind::Backend => ("backend", None, None),
    };
    let frontend = kind == "frontend";
    let style = css
        .filter(|s| !s.is_empty())
        .map(|css| copy(css, &output.id, sources))
        .transpose()?;
    let asset_directory = if output.assets.is_empty() {
        None
    } else {
        let destination = format!("{}/assets", output.id);
        sources.directory(&destination)?;
        for pattern in output.assets {
            assets.push(Asset {
                pattern,
                destination: destination.clone(),
            });
        }
        Some(destination)
    };
    Ok(ManifestPlugin {
        name: output.name.unwrap_or_else(|| output.id.clone()),
        id: output.id,
        kind,
        entrypoint,
        style,
        backend: frontend.then(|| {
            backend
                .filter(|s| !s.is_empty())
                .map(|id| BackendReference { id })
        }),
        runtime: (!frontend).then_some("javascript"),
        assets: asset_directory,
    })
}

pub(super) fn package_plan(
    config: PackageConfig,
    outputs: Vec<BuildOutput>,
) -> Result<PackagePlan, String> {
    unique_ids(outputs.iter().map(|output| output.id.as_str()))?;
    let mut sources = PackageSources::new()?;
    let mut assets = Vec::new();
    let plugins = outputs
        .into_iter()
        .map(|output| plan_output(output, &mut sources, &mut assets))
        .collect::<Result<_, _>>()?;
    Ok(PackagePlan {
        manifest: Manifest {
            id: config.id,
            name: config.name,
            version: config.version,
            description: config.description,
            author: config.author,
            links: config.links,
            plugins,
        },
        sources,
        assets,
    })
}
