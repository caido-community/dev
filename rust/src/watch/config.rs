use std::path::{Path, PathBuf};

use crate::package::PackageConfig;

/// Resolved watch setup derived once from validated adapter metadata.
pub(super) struct WatchConfig {
    pub id: String,
    pub roots: Vec<PathBuf>,
    pub port: u16,
    pub metadata: PackageConfig,
}

impl WatchConfig {
    pub fn resolve(cwd: &Path, metadata: PackageConfig) -> Result<Self, String> {
        let roots = metadata
            .plugins
            .iter()
            .map(|plugin| crate::filesystem::absolute(&cwd.join(&plugin.root)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            id: metadata.id.clone(),
            roots,
            port: metadata
                .watch
                .as_ref()
                .and_then(|watch| watch.port)
                .unwrap_or(3000),
            metadata,
        })
    }
}

pub(super) fn config_path(cwd: &Path, custom: Option<&str>) -> Result<String, String> {
    let path = match custom {
        Some(custom) => crate::filesystem::absolute(Path::new(custom))?,
        None => cwd.join("caido.config.ts"),
    };
    path.into_os_string()
        .into_string()
        .map_err(|_| "Config path is not valid UTF-8".into())
}

pub(super) fn watch_files(cwd: &Path, config_path: Option<&str>) -> Vec<PathBuf> {
    vec![
        cwd.join(config_path.unwrap_or("caido.config.ts")),
        cwd.join("package.json"),
        cwd.join("README.md"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_metadata_rejects_invalid_ports_and_missing_fields() {
        let cwd = std::env::current_dir().unwrap();
        let metadata = json!({"id":"test", "plugins":[]});
        let decode = |value| serde_json::from_value::<PackageConfig>(value);
        assert_eq!(
            WatchConfig::resolve(&cwd, decode(metadata.clone()).unwrap())
                .unwrap()
                .port,
            3000
        );
        let mut ephemeral = metadata.clone();
        ephemeral["watch"] = json!({"port":0});
        assert_eq!(
            WatchConfig::resolve(&cwd, decode(ephemeral).unwrap())
                .unwrap()
                .port,
            0
        );
        let mut configured = metadata.clone();
        configured["watch"] = json!({"port":4567});
        assert_eq!(
            WatchConfig::resolve(&cwd, decode(configured).unwrap())
                .unwrap()
                .port,
            4567
        );
        for value in [json!(-1), json!(1.5), json!(65536), json!("3000")] {
            let mut invalid = metadata.clone();
            invalid["watch"] = json!({"port":value});
            assert!(decode(invalid).is_err());
        }
        assert!(decode(json!({"plugins":[]})).is_err());
        assert!(decode(json!({"id":"test", "plugins":[{}]})).is_err());
    }
}
