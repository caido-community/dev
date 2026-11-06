mod input;
mod native;
mod plan;

#[cfg(test)]
mod input_tests;

pub(crate) use input::{BuildOutput, PackageConfig};
pub(crate) use native::assemble;
use plan::package_plan;
use std::collections::HashSet;
use typed_path::{Utf8UnixComponent, Utf8UnixPath};

struct Copy {
    source: String,
    destination: String,
}

struct Asset {
    pattern: String,
    destination: String,
}

#[derive(Clone)]
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

#[derive(Default)]
struct PackageSources {
    copies: Vec<Copy>,
    files: HashSet<String>,
    directories: HashSet<String>,
}

impl PackageSources {
    fn new() -> Result<Self, String> {
        let mut sources = Self::default();
        sources.reserve("README.md")?;
        sources.reserve("manifest.json")?;
        Ok(sources)
    }

    fn directory(&mut self, path: &str) -> Result<(), String> {
        if !safe_package_path(path) {
            return Err(format!("Invalid package destination '{path}'"));
        }
        let key = path.to_lowercase();
        for parent in Utf8UnixPath::new(&key)
            .ancestors()
            .filter(|parent| !parent.as_str().is_empty())
        {
            if self.files.contains(parent.as_str()) {
                return Err(format!("Package file/directory collision '{path}'"));
            }
            self.directories.insert(parent.as_str().to_owned());
        }
        Ok(())
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
