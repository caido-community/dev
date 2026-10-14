use crate::package::safe_component;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

pub(crate) fn create(input: &str, contents: &[u8]) -> Result<Vec<u8>, String> {
    let entries: Vec<Value> =
        serde_json::from_str(input).map_err(|e| format!("Invalid ZIP entries: {e}"))?;
    let mut checked = Vec::with_capacity(entries.len());
    let mut names = HashSet::new();
    let mut total = 0usize;
    for entry in &entries {
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .ok_or("Expected ZIP entry path string")?;
        if !path.split('/').all(safe_component) {
            return Err(format!(
                "Invalid ZIP entry path '{path}': expected a safe relative file path"
            ));
        }
        if !names.insert(path.to_ascii_lowercase()) {
            return Err(format!("Duplicate ZIP entry path '{path}'"));
        }
        let length = entry
            .get("length")
            .and_then(Value::as_u64)
            .ok_or("Expected non-negative integer ZIP entry length")?;
        let length =
            usize::try_from(length).map_err(|_| "ZIP entry length exceeds addressable memory")?;
        let end = total
            .checked_add(length)
            .ok_or("ZIP entry lengths overflow")?;
        if end > contents.len() {
            return Err("ZIP entry lengths exceed contents length".into());
        }
        checked.push((path, total, end));
        total = end;
    }
    if total != contents.len() {
        return Err("ZIP entry lengths do not match contents length".into());
    }
    // Validate before writing so malformed input cannot yield a partial archive.
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (path, start, end) in checked {
        writer
            .start_file(path, options)
            .map_err(|e| format!("Could not create ZIP entry '{path}': {e}"))?;
        writer
            .write_all(&contents[start..end])
            .map_err(|e| format!("Could not write ZIP entry '{path}': {e}"))?;
    }
    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|e| format!("Could not finish ZIP: {e}"))
}
