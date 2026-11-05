use crate::package::safe_package_path;
use std::collections::HashSet;
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

pub(crate) fn create_files(files: &[crate::package::PackageFile]) -> Result<Vec<u8>, String> {
    let entries: Vec<_> = files
        .iter()
        .map(|file| (file.path.as_str(), file.bytes.as_slice()))
        .collect();
    let mut names = HashSet::new();
    for (path, _) in &entries {
        if !safe_package_path(path) {
            return Err(format!(
                "Invalid ZIP entry path '{path}': expected a safe relative file path"
            ));
        }
        if !names.insert(path.to_ascii_lowercase()) {
            return Err(format!("Duplicate ZIP entry path '{path}'"));
        }
    }
    write_entries(&entries)
}

fn write_entries(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    // Validate before writing so malformed input cannot yield a partial archive.
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for &(path, bytes) in entries {
        writer
            .start_file(path, options)
            .map_err(|e| format!("Could not create ZIP entry '{path}': {e}"))?;
        writer
            .write_all(bytes)
            .map_err(|e| format!("Could not write ZIP entry '{path}': {e}"))?;
    }
    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|e| format!("Could not finish ZIP: {e}"))
}
