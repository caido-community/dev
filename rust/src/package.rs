use serde_json::{json, Value};
use std::collections::HashSet;
use wasm_bindgen::{prelude::*, JsCast};

fn error(message: impl AsRef<str>) -> JsValue {
    JsValue::from_str(message.as_ref())
}

async fn invoke(callback: &js_sys::Function, argument: &str) -> Result<JsValue, JsValue> {
    let pending = callback.call1(&JsValue::UNDEFINED, &JsValue::from_str(argument))?;
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&pending)).await
}

fn reserve(
    path: &str,
    files: &mut HashSet<String>,
    directories: &mut HashSet<String>,
) -> Result<(), JsValue> {
    if !path.split('/').all(safe_component) {
        return Err(error(format!("Invalid package destination '{path}'")));
    }
    let key = path.to_lowercase();
    if directories.contains(&key) || !files.insert(key.clone()) {
        return Err(error(format!("Duplicate package destination '{path}'")));
    }
    let mut parent = key.as_str();
    while let Some((prefix, _)) = parent.rsplit_once('/') {
        if files.contains(prefix) {
            return Err(error(format!("Package file/directory collision '{path}'")));
        }
        directories.insert(prefix.to_owned());
        parent = prefix;
    }
    Ok(())
}

pub(crate) async fn assemble(
    input: &str,
    read_file: js_sys::Function,
    expand_assets: js_sys::Function,
    read_asset: js_sys::Function,
    encode: js_sys::Function,
    validate_manifest: js_sys::Function,
) -> Result<JsValue, JsValue> {
    let planned: Value =
        serde_json::from_str(&plan(input).map_err(error)?).map_err(|e| error(e.to_string()))?;
    let manifest = &planned["manifest"];
    let manifest_text = serde_json::to_string_pretty(manifest).map_err(|e| error(e.to_string()))?;
    let accepted =
        validate_manifest.call1(&JsValue::UNDEFINED, &JsValue::from_str(&manifest_text))?;
    if accepted.as_bool() != Some(true) {
        return Err(error(format!("Manifest is not valid:{manifest_text}")));
    }
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut files = HashSet::new();
    let mut directories = HashSet::new();
    reserve("README.md", &mut files, &mut directories)?;
    reserve("manifest.json", &mut files, &mut directories)?;
    for copy in planned["copies"]
        .as_array()
        .ok_or_else(|| error("Invalid copies"))?
    {
        let destination = string(copy, "destination").map_err(error)?;
        reserve(destination, &mut files, &mut directories)?;
        sources.push((
            destination.to_owned(),
            string(copy, "source").map_err(error)?.to_owned(),
        ));
    }
    for asset in planned["assets"]
        .as_array()
        .ok_or_else(|| error("Invalid assets"))?
    {
        let expanded = invoke(&expand_assets, string(asset, "pattern").map_err(error)?).await?;
        if !js_sys::Array::is_array(&expanded) {
            return Err(error("Invalid asset expansion"));
        }
        for item in js_sys::Array::from(&expanded).iter() {
            let relative = js_sys::Reflect::get(&item, &JsValue::from_str("relative"))?
                .as_string()
                .ok_or_else(|| error("Invalid asset relative path"))?;
            let source = js_sys::Reflect::get(&item, &JsValue::from_str("source"))?
                .as_string()
                .ok_or_else(|| error("Invalid asset source"))?;
            let destination = format!(
                "{}/{}",
                string(asset, "destination").map_err(error)?,
                relative
            );
            reserve(&destination, &mut files, &mut directories)?;
            sources.push((destination, source));
        }
    }
    let read_bytes = |value: JsValue| -> Result<Vec<u8>, JsValue> {
        if !value.is_instance_of::<js_sys::Uint8Array>() {
            return Err(error("Expected file bytes"));
        }
        Ok(js_sys::Uint8Array::new(&value).to_vec())
    };
    let readme = read_bytes(invoke(&read_file, "README.md").await?)?;
    let content =
        String::from_utf8(readme).map_err(|e| error(format!("Invalid README.md: {e}")))?;
    let transformed = crate::bundle_readme(content, read_asset, encode).await?;
    let transformed: Value =
        serde_json::from_str(&transformed).map_err(|e| error(e.to_string()))?;
    let mut entries = vec![
        (
            "README.md".to_owned(),
            string(&transformed, "content")
                .map_err(error)?
                .as_bytes()
                .to_vec(),
        ),
        ("manifest.json".to_owned(), manifest_text.into_bytes()),
    ];
    for (destination, source) in sources {
        entries.push((destination, read_bytes(invoke(&read_file, &source).await?)?));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let metadata: Vec<Value> = entries
        .iter()
        .map(|(path, bytes)| json!({"path":path,"length":bytes.len()}))
        .collect();
    let contents: Vec<u8> = entries
        .iter()
        .flat_map(|(_, bytes)| bytes.iter().copied())
        .collect();
    let zip = crate::archive::create(
        &serde_json::to_string(&metadata).map_err(|e| error(e.to_string()))?,
        &contents,
    )
    .map_err(error)?;
    let result = js_sys::Object::new();
    let output = js_sys::Array::new();
    for (path, bytes) in entries {
        let file = js_sys::Object::new();
        js_sys::Reflect::set(&file, &"path".into(), &path.into())?;
        js_sys::Reflect::set(
            &file,
            &"bytes".into(),
            &js_sys::Uint8Array::from(bytes.as_slice()),
        )?;
        output.push(&file);
    }
    let messages = js_sys::Array::new();
    for key in ["messages", "warnings"] {
        if let Some(values) = transformed[key].as_array() {
            for value in values {
                if let Some(text) = value.as_str() {
                    messages.push(&text.into());
                }
            }
        }
    }
    js_sys::Reflect::set(&result, &"files".into(), &output)?;
    js_sys::Reflect::set(
        &result,
        &"zip".into(),
        &js_sys::Uint8Array::from(zip.as_slice()),
    )?;
    js_sys::Reflect::set(&result, &"messages".into(), &messages)?;
    Ok(result.into())
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

fn copy(
    source: &str,
    id: &str,
    copies: &mut Vec<Value>,
    destinations: &mut HashSet<String>,
) -> Result<String, String> {
    let basename = source.rsplit(['/', '\\']).next().unwrap_or("");
    if !safe_component(basename) {
        return Err(format!("Invalid build output filename '{source}'"));
    }
    let destination = format!("{id}/{basename}");
    if !destinations.insert(destination.to_ascii_lowercase()) {
        return Err(format!("Duplicate package destination '{destination}'"));
    }
    copies.push(json!({"source":source,"destination":destination}));
    Ok(destination)
}

pub(crate) fn plan(input: &str) -> Result<String, String> {
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
                "links".to_owned(),
                config.get("links").cloned().unwrap_or_else(|| json!({})),
            );
            fields.insert("plugins".to_owned(), json!([]));
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
        let kind = string(output, "kind")?;
        if !matches!(kind, "frontend" | "backend") {
            return Err(format!("Unsupported build output kind '{kind}'"));
        }
        let id = string(output, "id")?;
        let entrypoint = copy(
            string(output, "fileName")?,
            id,
            &mut copies,
            &mut destinations,
        )?;
        let mut plugin = json!({"id":id,"kind":kind,"name":optional_string(output,"name")?.unwrap_or(id),"entrypoint":entrypoint});
        if kind == "frontend" {
            if let Some(css) = optional_string(output, "cssFileName")?.filter(|v| !v.is_empty()) {
                plugin["style"] = Value::from(copy(css, id, &mut copies, &mut destinations)?);
            }
            plugin["backend"] =
                match optional_string(output, "backendId")?.filter(|v| !v.is_empty()) {
                    Some(id) => json!({"id":id}),
                    None => Value::Null,
                };
        } else {
            plugin["runtime"] = Value::from("javascript");
        }
        let patterns = output
            .get("assets")
            .and_then(Value::as_array)
            .ok_or("Expected build output assets array")?;
        if !patterns.is_empty() {
            let destination = format!("{id}/assets");
            if destinations.contains(&destination.to_ascii_lowercase()) {
                return Err(format!(
                    "Assets directory collides with package destination '{destination}'"
                ));
            }
            plugin["assets"] = Value::from(destination.clone());
            for pattern in patterns {
                assets.push(json!({"pattern":pattern.as_str().ok_or("Expected asset pattern string")?,"destination":destination}));
            }
        }
        plugins.push(plugin);
    }
    validate_ids(&plugins)?;
    manifest["plugins"] = Value::from(plugins);
    Ok(json!({"manifest":manifest,"copies":copies,"assets":assets}).to_string())
}
