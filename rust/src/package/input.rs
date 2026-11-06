use super::safe_component;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashSet;

fn id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let id = String::deserialize(deserializer)?;
    if !safe_component(&id) {
        return Err(de::Error::custom(format!(
            "Invalid plugin id '{id}': expected a safe single path component"
        )));
    }
    Ok(id)
}

fn optional_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let value = Option::<String>::deserialize(deserializer)?;
    if let Some(id) = value.as_deref().filter(|id| !id.is_empty()) {
        if !safe_component(id) {
            return Err(de::Error::custom(format!("Invalid plugin id '{id}'")));
        }
    }
    Ok(value)
}

fn plugins<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PluginConfig>, D::Error> {
    let plugins = Vec::<PluginConfig>::deserialize(deserializer)?;
    unique_ids(plugins.iter().map(|plugin| plugin.id.as_str())).map_err(de::Error::custom)?;
    Ok(plugins)
}

pub(super) fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.to_ascii_lowercase()) {
            return Err(format!("Duplicate plugin id '{id}'"));
        }
    }
    Ok(())
}

/// Serializable config fields needed by native orchestration and packaging.
/// Executable build options remain in Node; path components are checked here.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(remote = "Self")]
pub(crate) struct PackageConfig {
    #[serde(skip)]
    metadata: Value,
    #[serde(deserialize_with = "id")]
    pub(crate) id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) author: Option<Author>,
    #[serde(default)]
    pub(crate) links: Links,
    #[serde(deserialize_with = "plugins")]
    pub(crate) plugins: Vec<PluginConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) watch: Option<WatchConfig>,
}

impl<'de> Deserialize<'de> for PackageConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let metadata = Value::deserialize(deserializer)?;
        let mut config = Self::deserialize(&metadata).map_err(de::Error::custom)?;
        config.metadata = metadata;
        Ok(config)
    }
}

impl Serialize for PackageConfig {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Node compares the complete projection before building. Preserve fields
        // unused by Rust and the distinction between omitted and null values.
        self.metadata.serialize(serializer)
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_complete_adapter_projection() {
        for backend in [None, Some(Value::Null), Some(json!({"id": "server"}))] {
            let mut metadata = json!({
                "id": "package", "name": "Package", "plugins": [{
                    "id": "client", "kind": "frontend", "root": "/project/client",
                    "name": "Client", "assets": ["assets/**/*"]
                }]
            });
            if let Some(backend) = backend {
                metadata["plugins"][0]["backend"] = backend;
                metadata["links"] = json!({});
            }
            let config: PackageConfig = serde_json::from_value(metadata.clone()).unwrap();
            assert_eq!(config.plugins[0].root, "/project/client");
            assert_eq!(serde_json::to_value(config).unwrap(), metadata);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(crate) struct Author {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub(crate) struct Links {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sponsor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(crate) struct WatchConfig {
    pub(crate) port: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(crate) struct PluginConfig {
    #[serde(deserialize_with = "id")]
    pub(crate) id: String,
    pub(crate) root: String,
    pub(crate) backend: Option<BackendReference>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub(crate) struct BackendReference {
    #[serde(deserialize_with = "id")]
    pub(crate) id: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum BuildKind {
    Frontend {
        #[serde(rename = "cssFileName")]
        css: Option<String>,
        #[serde(rename = "backendId", default, deserialize_with = "optional_id")]
        backend: Option<String>,
    },
    Backend,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildOutput {
    #[serde(flatten)]
    pub(crate) kind: BuildKind,
    #[serde(deserialize_with = "id")]
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) file_name: String,
    pub(crate) assets: Vec<String>,
}
