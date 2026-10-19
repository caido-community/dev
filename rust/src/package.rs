mod wasm;

pub(crate) use wasm::assemble;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use typed_path::{Utf8UnixComponent, Utf8UnixPath, Utf8WindowsPath};

#[derive(Serialize)]
struct PackagePlan {
    // Supplied manifests retain unknown fields and insertion order. The host's
    // manifest schema remains authoritative for their validation.
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

#[derive(Deserialize, Serialize)]
pub(crate) struct ReadmeResult {
    pub(crate) content: String,
    pub(crate) warnings: Vec<String>,
    #[serde(default)]
    pub(crate) messages: Vec<String>,
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

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum BuildKind {
    Frontend,
    Backend,
}

struct BuildOutput<'a> {
    kind: BuildKind,
    id: &'a str,
    name: &'a str,
    file_name: &'a str,
    css: Option<&'a str>,
    backend: Option<&'a str>,
    assets: Vec<&'a str>,
}

impl<'a> BuildOutput<'a> {
    fn parse(value: &'a Value) -> Result<Self, String> {
        let kind = match string(value, "kind")? {
            "frontend" => BuildKind::Frontend,
            "backend" => BuildKind::Backend,
            kind => return Err(format!("Unsupported build output kind '{kind}'")),
        };
        let id = string(value, "id")?;
        let frontend = matches!(kind, BuildKind::Frontend);
        Ok(Self {
            kind,
            id,
            name: optional_string(value, "name")?.unwrap_or(id),
            file_name: string(value, "fileName")?,
            css: if frontend {
                optional_string(value, "cssFileName")?.filter(|s| !s.is_empty())
            } else {
                None
            },
            backend: if frontend {
                optional_string(value, "backendId")?.filter(|s| !s.is_empty())
            } else {
                None
            },
            assets: value
                .get("assets")
                .and_then(Value::as_array)
                .ok_or("Expected build output assets array")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| "Expected asset pattern string".into())
                })
                .collect::<Result<_, String>>()?,
        })
    }
}

#[derive(Serialize)]
struct BackendReference<'a> {
    id: &'a str,
}

// Field order matches the existing generated manifest entries.
#[derive(Serialize)]
struct ManifestPlugin<'a> {
    id: &'a str,
    kind: BuildKind,
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
    let frontend = matches!(output.kind, BuildKind::Frontend);
    let style = output
        .css
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
        kind: output.kind,
        name: output.name,
        entrypoint,
        style,
        backend: frontend.then(|| output.backend.map(|id| BackendReference { id })),
        runtime: (!frontend).then_some("javascript"),
        assets: asset_directory,
    })
    .map_err(|e| e.to_string())
}

fn package_plan(input: &str) -> Result<PackagePlan, String> {
    // Parse only the fields packaging owns; config and supplied manifests may
    // contain additional fields interpreted by their existing host schemas.
    let input: Value =
        serde_json::from_str(input).map_err(|e| format!("Invalid package input: {e}"))?;
    let config = input
        .get("config")
        .filter(|v| v.is_object())
        .ok_or("Expected config object")?;
    validate_id(string(config, "id")?)?;
    let configured = config
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or("Expected config.plugins array")?;
    validate_ids(configured)?;
    let outputs = input
        .get("buildOutputs")
        .and_then(Value::as_array)
        .ok_or("Expected buildOutputs array")?;
    validate_ids(outputs)?;
    let mut manifest = input
        .get("manifest")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| {
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
            BuildOutput::parse(output)?,
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

pub(crate) fn plan(input: &str) -> Result<String, String> {
    // Preserve the legacy JSON object's field order as well as its shape.
    let plan = package_plan(input)?;
    Ok(json!({"manifest":plan.manifest,"copies":plan.copies,"assets":plan.assets}).to_string())
}
