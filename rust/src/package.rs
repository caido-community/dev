mod native;

pub(crate) use native::assemble;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use typed_path::{Utf8UnixComponent, Utf8UnixPath, Utf8WindowsPath};

#[derive(Serialize)]
struct PackagePlan {
    // Legacy supplied manifests retain unknown fields and insertion order.
    manifest: Value,
    copies: Vec<Copy>,
    assets: Vec<Asset>,
}

#[derive(Serialize)]
struct Copy {
    source: String,
    destination: String,
}

#[derive(Serialize)]
struct Asset {
    pattern: String,
    destination: String,
}

#[derive(Clone, Deserialize)]
struct ExpandedAsset {
    relative: String,
    source: String,
}

pub(crate) struct PackageFile {
    pub(crate) path: String,
    pub(crate) bytes: Vec<u8>,
}

struct PreparedPackage {
    files: Vec<PackageFile>,
    zip: Vec<u8>,
    messages: Vec<String>,
}

use crate::readme::ReadmeResult;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageInput {
    config: Value,
    build_outputs: Vec<Value>,
    manifest: Option<Value>,
}

#[derive(Default)]
struct PackageSources {
    copies: Vec<Copy>,
    files: HashSet<String>,
    directories: HashSet<String>,
}

impl PackageSources {
    fn new(copies: Vec<Copy>) -> Result<Self, String> {
        let mut sources = Self::default();
        sources.reserve("README.md")?;
        sources.reserve("manifest.json")?;
        for copy in copies {
            sources.add(copy)?;
        }
        Ok(sources)
    }

    fn reserve(&mut self, path: &str) -> Result<(), String> {
        if !safe_package_path(path) {
            return Err(format!("Invalid package destination '{path}'"));
        }
        let key = path.to_lowercase();
        if self.directories.contains(&key) || !self.files.insert(key.clone()) {
            return Err(format!("Duplicate package destination '{path}'"));
        }
        for parent in Utf8UnixPath::new(&key)
            .ancestors()
            .skip(1)
            .filter(|parent| !parent.as_str().is_empty())
        {
            if self.files.contains(parent.as_str()) {
                return Err(format!("Package file/directory collision '{path}'"));
            }
            self.directories.insert(parent.as_str().to_owned());
        }
        Ok(())
    }

    fn add(&mut self, copy: Copy) -> Result<(), String> {
        self.reserve(&copy.destination)?;
        self.copies.push(copy);
        Ok(())
    }

    fn expand(&mut self, asset: &Asset, expanded: ExpandedAsset) -> Result<(), String> {
        self.add(Copy {
            destination: format!("{}/{}", asset.destination, expanded.relative),
            source: expanded.source,
        })
    }
}

fn package_files(manifest: String, readme: &ReadmeResult) -> Vec<PackageFile> {
    vec![
        PackageFile {
            path: "README.md".into(),
            bytes: readme.content.as_bytes().to_vec(),
        },
        PackageFile {
            path: "manifest.json".into(),
            bytes: manifest.into_bytes(),
        },
    ]
}

fn prepare(mut files: Vec<PackageFile>, readme: ReadmeResult) -> Result<PreparedPackage, String> {
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let zip = crate::archive::create_files(&files)?;
    let mut messages = readme.messages;
    messages.extend(readme.warnings);
    Ok(PreparedPackage {
        files,
        zip,
        messages,
    })
}

pub(crate) fn safe_package_path(path: &str) -> bool {
    let parsed = Utf8UnixPath::new(path);
    !path.is_empty()
        && parsed.components().all(|component| {
            matches!(component, Utf8UnixComponent::Normal(name) if safe_component(name))
        })
        // Package paths must already be canonical; never silently normalize input.
        && parsed.normalize().as_str() == path
}

pub(crate) fn safe_component(id: &str) -> bool {
    !id.is_empty()
        && !id.chars().any(char::is_control)
        && sanitize_filename::is_sanitized_with_options(
            id,
            sanitize_filename::OptionsForCheck {
                windows: true,
                truncate: false,
            },
        )
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Expected string '{key}'"))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("Expected string '{key}'")),
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if safe_component(id) {
        Ok(())
    } else {
        Err(format!(
            "Invalid plugin id '{id}': expected a safe single path component"
        ))
    }
}

fn validate_ids(plugins: &[Value]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for plugin in plugins {
        let id = string(plugin, "id")?;
        validate_id(id)?;
        if !ids.insert(id.to_ascii_lowercase()) {
            return Err(format!("Duplicate plugin id '{id}'"));
        }
        if let Some(backend) = plugin.get("backend").filter(|v| !v.is_null()) {
            validate_id(string(backend, "id")?)?;
        }
        if let Some(id) = optional_string(plugin, "backendId")? {
            if !id.is_empty() {
                validate_id(id)?;
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum BuildKind<'a> {
    Frontend {
        #[serde(rename = "cssFileName", borrow)]
        css: Option<&'a str>,
        #[serde(rename = "backendId", borrow)]
        backend: Option<&'a str>,
    },
    Backend,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildOutput<'a> {
    #[serde(flatten, borrow)]
    kind: BuildKind<'a>,
    id: &'a str,
    name: Option<&'a str>,
    file_name: &'a str,
    assets: Vec<&'a str>,
}

#[derive(Serialize)]
struct BackendReference<'a> {
    id: &'a str,
}

// Field order matches the existing generated manifest entries.
#[derive(Serialize)]
struct ManifestPlugin<'a> {
    id: &'a str,
    kind: &'static str,
    name: &'a str,
    entrypoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<Option<BackendReference<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<String>,
}

fn copy(
    source: &str,
    id: &str,
    copies: &mut Vec<Copy>,
    destinations: &mut HashSet<String>,
) -> Result<String, String> {
    // Windows parsing recognizes both separators, independent of the WASM host.
    let basename = Utf8WindowsPath::new(source).file_name().unwrap_or("");
    if !safe_component(basename) || !source.ends_with(basename) {
        return Err(format!("Invalid build output filename '{source}'"));
    }
    let destination = format!("{id}/{basename}");
    if !destinations.insert(destination.to_ascii_lowercase()) {
        return Err(format!("Duplicate package destination '{destination}'"));
    }
    copies.push(Copy {
        source: source.into(),
        destination: destination.clone(),
    });
    Ok(destination)
}

fn plan_output(
    output: BuildOutput<'_>,
    copies: &mut Vec<Copy>,
    assets: &mut Vec<Asset>,
    destinations: &mut HashSet<String>,
) -> Result<Value, String> {
    let entrypoint = copy(output.file_name, output.id, copies, destinations)?;
    let (kind, css, backend) = match output.kind {
        BuildKind::Frontend { css, backend } => ("frontend", css, backend),
        BuildKind::Backend => ("backend", None, None),
    };
    let frontend = kind == "frontend";
    let style = css
        .filter(|s| !s.is_empty())
        .map(|css| copy(css, output.id, copies, destinations))
        .transpose()?;
    let asset_directory = if output.assets.is_empty() {
        None
    } else {
        let destination = format!("{}/assets", output.id);
        if destinations.contains(&destination.to_ascii_lowercase()) {
            return Err(format!(
                "Assets directory collides with package destination '{destination}'"
            ));
        }
        for pattern in output.assets {
            assets.push(Asset {
                pattern: pattern.into(),
                destination: destination.clone(),
            });
        }
        Some(destination)
    };
    serde_json::to_value(ManifestPlugin {
        id: output.id,
        kind,
        name: output.name.unwrap_or(output.id),
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
    .map_err(|e| e.to_string())
}

fn package_plan(input: PackageInput) -> Result<PackagePlan, String> {
    // Inspect only the fields packaging owns; user config is validated before builds.
    let config = Some(&input.config)
        .filter(|v| v.is_object())
        .ok_or("Expected config object")?;
    validate_id(string(config, "id")?)?;
    let configured = config
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or("Expected config.plugins array")?;
    validate_ids(configured)?;
    let outputs = &input.build_outputs;
    validate_ids(outputs)?;
    let mut manifest = input.manifest.filter(|v| v.is_object()).unwrap_or_else(|| {
        let mut fields = serde_json::Map::new();
        for key in ["id", "name", "version", "description", "author"] {
            if let Some(value) = config.get(key) {
                fields.insert(key.to_owned(), value.clone());
            }
        }
        fields.insert(
            "links".into(),
            config.get("links").cloned().unwrap_or_else(|| json!({})),
        );
        fields.insert("plugins".into(), json!([]));
        Value::Object(fields)
    });
    if let Some(id) = manifest.get("id") {
        validate_id(id.as_str().ok_or("Expected manifest.id string")?)?;
    }
    let mut plugins = manifest
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or("Expected manifest.plugins array")?
        .clone();
    validate_ids(&plugins)?;
    let mut copies = Vec::new();
    let mut assets = Vec::new();
    let mut destinations = HashSet::new();
    for output in outputs {
        plugins.push(plan_output(
            BuildOutput::deserialize(output).map_err(|e| format!("Invalid build output: {e}"))?,
            &mut copies,
            &mut assets,
            &mut destinations,
        )?);
    }
    validate_ids(&plugins)?;
    manifest["plugins"] = Value::from(plugins);
    Ok(PackagePlan {
        manifest,
        copies,
        assets,
    })
}
