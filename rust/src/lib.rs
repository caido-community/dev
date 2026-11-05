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
) -> Result<JsValue, JsValue> {
    let result = bundle_readme_result(content, read_asset, encode).await?;
    serde_wasm_bindgen::to_value(&result)
        .map_err(|error| JsValue::from_str(&format!("Invalid README output: {error}")))
}

async fn bundle_readme_result(
    content: String,
    read_asset: js_sys::Function,
    encode: js_sys::Function,
) -> Result<readme::ReadmeResult, JsValue> {
    let urls = readme::asset_urls(&content).map_err(|error| JsValue::from_str(&error))?;
    let mut replacements = std::collections::HashMap::new();
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
        replacements.insert(url, converted.url);
        messages.push(converted.message);
    }
    let mut result = readme::transform_content(&content, &replacements)
        .map_err(|error| JsValue::from_str(&error))?;
    result.messages = messages;
    Ok(result)
}

use wasm_bindgen::prelude::*;

/// Parse user arguments (process.argv.slice(2)) without performing any I/O.
#[wasm_bindgen]
pub fn parse_cli(args: JsValue) -> Result<JsValue, JsValue> {
    let args = serde_wasm_bindgen::from_value(args)
        .map_err(|error| JsValue::from_str(&format!("Invalid CLI arguments: {error}")))?;
    let response = cli::parse(args).map_err(|error| JsValue::from_str(&error))?;
    serde::Serialize::serialize(
        &response,
        &serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true),
    )
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

/// Assemble a complete package before the host changes its output directory.
#[wasm_bindgen]
pub fn package_plugin(input: JsValue, preloaded: JsValue) -> Result<JsValue, JsValue> {
    package::assemble(input, preloaded)
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
