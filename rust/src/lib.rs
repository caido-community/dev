mod archive;
mod cli;
mod package;
mod readme;
mod readme_image;

#[wasm_bindgen]
pub fn readme_assets(content: &str) -> Result<String, JsValue> {
    readme::assets(content).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn transform_readme(content: &str, replacements: &str) -> Result<String, JsValue> {
    readme::transform(content, replacements).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn readme_local_path(url: &str) -> String {
    readme::local_path(url)
}

#[wasm_bindgen]
pub async fn convert_readme_image(
    input: Vec<u8>,
    url: String,
    encode: js_sys::Function,
) -> Result<JsValue, JsValue> {
    readme_image::convert(&input, &url, &encode)
        .await?
        .into_js()
}

/// Rust owns the asset traversal; the host reads paths and invokes the encoder.
#[wasm_bindgen]
pub async fn bundle_readme(
    content: String,
    read_asset: js_sys::Function,
    encode: js_sys::Function,
) -> Result<String, JsValue> {
    let assets = readme::assets(&content).map_err(|error| JsValue::from_str(&error))?;
    let urls: Vec<String> =
        serde_json::from_str(&assets).map_err(|error| JsValue::from_str(&error.to_string()))?;
    let mut replacements = serde_json::Map::new();
    let mut messages = Vec::new();
    for url in urls {
        let pending = read_asset.call2(
            &JsValue::UNDEFINED,
            &JsValue::from_str(&readme::local_path(&url)),
            &JsValue::from_str(&url),
        )?;
        let bytes =
            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&pending)).await?;
        if !bytes.is_instance_of::<js_sys::Uint8Array>() {
            return Err(JsValue::from_str(&format!(
                "Invalid README asset bytes: {url}"
            )));
        }
        let converted =
            readme_image::convert(&js_sys::Uint8Array::new(&bytes).to_vec(), &url, &encode).await?;
        replacements.insert(url, serde_json::Value::String(converted.url));
        messages.push(converted.message);
    }
    let transformed = readme::transform(
        &content,
        &serde_json::Value::Object(replacements).to_string(),
    )
    .map_err(|error| JsValue::from_str(&error))?;
    let mut result: serde_json::Value = serde_json::from_str(&transformed)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    result["messages"] = serde_json::json!(messages);
    Ok(result.to_string())
}

use wasm_bindgen::prelude::*;

/// Parse user arguments (process.argv.slice(2)) without performing any I/O.
#[wasm_bindgen]
pub fn parse_cli(args_json: &str) -> Result<String, JsValue> {
    cli::parse(args_json).map_err(|error| JsValue::from_str(&error))
}

/// Assemble a complete package before the host changes its output directory.
#[wasm_bindgen]
pub async fn package_plugin(
    input: String,
    read_file: js_sys::Function,
    expand_assets: js_sys::Function,
    read_asset: js_sys::Function,
    encode: js_sys::Function,
    validate_manifest: js_sys::Function,
) -> Result<JsValue, JsValue> {
    package::assemble(
        &input,
        read_file,
        expand_assets,
        read_asset,
        encode,
        validate_manifest,
    )
    .await
}

/// Plan copies and manifest entries; filesystem and glob expansion stay in JS.
#[wasm_bindgen]
pub fn plan_package(input_json: &str) -> Result<String, JsValue> {
    package::plan(input_json).map_err(|error| JsValue::from_str(&error))
}

/// Create a stored ZIP from entry metadata and concatenated file bytes.
#[wasm_bindgen]
pub fn create_zip(entries_json: &str, contents: &[u8]) -> Result<Vec<u8>, JsValue> {
    archive::create(entries_json, contents).map_err(|error| JsValue::from_str(&error))
}
