use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

pub(crate) const MAX_LOCAL_IMAGE_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn absolute(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn key(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.as_os_str().to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

pub(crate) fn contains(root: &Path, path: &Path) -> bool {
    key(path).starts_with(key(root))
}

/// Reject leaf symlinks and escapes through parent directories. These checks do not
/// eliminate races with concurrent filesystem changes.
pub(crate) fn checked_source(
    root: &Path,
    source: &Path,
    allow_root: bool,
) -> Result<PathBuf, String> {
    let original = absolute(root)?;
    let root =
        fs::canonicalize(&original).map_err(|e| format!("Cannot resolve project root: {e}"))?;
    let path = absolute(&original.join(source))?;
    let invalid = || {
        format!(
            "Package source path escapes project root or is a symbolic link: {}",
            source.display()
        )
    };
    if !contains(&original, &path) && !contains(&root, &path) {
        return Err(invalid());
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|e| format!("Cannot inspect package source '{}': {e}", source.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(invalid());
    }
    let real = fs::canonicalize(&path)
        .map_err(|e| format!("Cannot resolve package source '{}': {e}", source.display()))?;
    if !contains(&root, &real) || (!allow_root && key(&root) == key(&real)) {
        return Err(invalid());
    }
    Ok(real)
}

/// Validate the output before any input reads and again just before replacement.
pub(crate) fn check_output(root: &Path, plugin_roots: &[PathBuf]) -> Result<(), String> {
    let real_root =
        fs::canonicalize(root).map_err(|e| format!("Cannot resolve project root: {e}"))?;
    if key(root) != key(&real_root) {
        return Err("Project root changed while packaging".into());
    }
    let dist = root.join("dist");
    match fs::symlink_metadata(&dist) {
        Ok(stats) => {
            if stats.file_type().is_symlink()
                || !stats.is_dir()
                || key(&fs::canonicalize(&dist).map_err(|e| e.to_string())?) != key(&dist)
            {
                return Err(
                    "Package output directory must be a real directory inside the project root"
                        .into(),
                );
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Cannot inspect package output directory: {e}")),
    }
    for plugin in plugin_roots {
        let path = absolute(&root.join(plugin))?;
        let real = fs::canonicalize(&path)
            .map_err(|e| format!("Cannot resolve plugin root '{}': {e}", plugin.display()))?;
        if contains(&dist, &path) || contains(&dist, &real) {
            return Err("Plugin roots must not be inside the package output directory".into());
        }
    }
    Ok(())
}

fn read_bounded(reader: &mut impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let count = chunk.len().min(limit + 1 - bytes.len());
        let n = match reader.read(&mut chunk[..count]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result.map_err(|e| e.to_string())?,
        };
        if n == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > limit {
            return Err("source limit exceeded".into());
        }
    }
}

pub(crate) fn read_asset(root: &Path, pathname: &str, url: &str) -> Result<Vec<u8>, String> {
    let path = checked_source(root, Path::new(pathname), false)
        .map_err(|e| format!("README asset path is invalid: {url}: {e}"))?;
    let oversized = || {
        format!("README image {url} exceeds the 64 MiB ({MAX_LOCAL_IMAGE_BYTES} byte) source limit")
    };
    let mut file = File::open(&path).map_err(|e| format!("Cannot open README image {url}: {e}"))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Cannot inspect README image {url}: {e}"))?;
    if !metadata.is_file() {
        return Err(format!("README image is not a regular file: {url}"));
    }
    if metadata.len() > MAX_LOCAL_IMAGE_BYTES as u64 {
        return Err(oversized());
    }
    read_bounded(&mut file, MAX_LOCAL_IMAGE_BYTES).map_err(|e| {
        if e == "source limit exceeded" {
            oversized()
        } else {
            format!("Cannot read README image {url}: {e}")
        }
    })
}

#[cfg(test)]
#[path = "filesystem/tests.rs"]
pub(crate) mod tests;
