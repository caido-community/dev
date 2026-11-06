use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TempDir(pub(crate) PathBuf);
impl TempDir {
    pub(crate) fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "caido-native-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn containment_and_root_policy() {
    let root = TempDir::new();
    let outside = TempDir::new();
    fs::write(root.0.join("input"), b"input").unwrap();
    fs::write(outside.0.join("secret"), b"secret").unwrap();
    assert!(checked_source(&root.0, Path::new("input"), false).is_ok());
    assert!(checked_source(&root.0, Path::new("."), false).is_err());
    assert!(checked_source(&root.0, Path::new("."), true).is_ok());
    assert!(checked_source(&root.0, &outside.0.join("secret"), false).is_err());
}

#[test]
fn bounded_reads_allow_limit_and_detect_growth() {
    assert_eq!(read_bounded(&mut &b"abcd"[..], 4).unwrap(), b"abcd");
    let mut source = &b"abcdefghi"[..];
    assert!(read_bounded(&mut source, 4).is_err());
    assert_eq!(source, b"fghi"); // At most limit + one bytes consumed.
}

#[test]
fn asset_source_limit_is_inclusive() {
    let root = TempDir::new();
    let path = root.0.join("asset");
    File::create(&path)
        .unwrap()
        .set_len(MAX_LOCAL_IMAGE_BYTES as u64)
        .unwrap();
    assert_eq!(
        read_asset(&root.0, "asset", "asset").unwrap().len(),
        MAX_LOCAL_IMAGE_BYTES
    );
    File::create(&path)
        .unwrap()
        .set_len(MAX_LOCAL_IMAGE_BYTES as u64 + 1)
        .unwrap();
    assert!(read_asset(&root.0, "asset", "asset")
        .unwrap_err()
        .contains("64 MiB"));
}

#[test]
fn unsafe_output_and_plugin_roots_are_rejected() {
    let root = TempDir::new();
    fs::write(root.0.join("dist"), b"sentinel").unwrap();
    assert!(check_output(&root.0, &[]).is_err());
    assert_eq!(fs::read(root.0.join("dist")).unwrap(), b"sentinel");
    fs::remove_file(root.0.join("dist")).unwrap();
    fs::create_dir_all(root.0.join("dist/plugin")).unwrap();
    assert!(check_output(&root.0, &[PathBuf::from("dist/plugin")]).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_and_parent_escapes_are_rejected() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new();
    let outside = TempDir::new();
    fs::write(root.0.join("real"), b"data").unwrap();
    fs::write(outside.0.join("secret"), b"secret").unwrap();
    symlink(root.0.join("real"), root.0.join("leaf")).unwrap();
    symlink(&outside.0, root.0.join("parent")).unwrap();
    assert!(checked_source(&root.0, Path::new("leaf"), false).is_err());
    assert!(checked_source(&root.0, Path::new("parent/secret"), false).is_err());
    symlink(&outside.0, root.0.join("dist")).unwrap();
    assert!(check_output(&root.0, &[]).is_err());
    assert_eq!(fs::read(outside.0.join("secret")).unwrap(), b"secret");
}
