use super::{
    package_files, package_plan, prepare, ExpandedAsset, PackageFile, PackageSources,
    PreparedPackage,
};
use wasm_bindgen::{prelude::*, JsCast};

fn error(message: impl AsRef<str>) -> JsValue {
    JsValue::from_str(message.as_ref())
}

async fn invoke(callback: &js_sys::Function, argument: &str) -> Result<JsValue, JsValue> {
    let pending = callback.call1(&JsValue::UNDEFINED, &JsValue::from_str(argument))?;
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&pending)).await
}

fn expanded_assets(value: JsValue) -> Result<Vec<ExpandedAsset>, JsValue> {
    if !js_sys::Array::is_array(&value) {
        return Err(error("Invalid asset expansion"));
    }
    js_sys::Array::from(&value)
        .iter()
        .map(|item| {
            let relative = js_sys::Reflect::get(&item, &"relative".into())?
                .as_string()
                .ok_or_else(|| error("Invalid asset relative path"))?;
            let source = js_sys::Reflect::get(&item, &"source".into())?
                .as_string()
                .ok_or_else(|| error("Invalid asset source"))?;
            Ok(ExpandedAsset { relative, source })
        })
        .collect()
}

async fn read_bytes(callback: &js_sys::Function, path: &str) -> Result<Vec<u8>, JsValue> {
    let value = invoke(callback, path).await?;
    if !value.is_instance_of::<js_sys::Uint8Array>() {
        return Err(error("Expected file bytes"));
    }
    Ok(js_sys::Uint8Array::new(&value).to_vec())
}

async fn read_files(
    callback: &js_sys::Function,
    sources: PackageSources,
    files: &mut Vec<PackageFile>,
) -> Result<(), JsValue> {
    for copy in sources.copies {
        files.push(PackageFile {
            path: copy.destination,
            bytes: read_bytes(callback, &copy.source).await?,
        });
    }
    Ok(())
}

fn into_js(package: PreparedPackage) -> Result<JsValue, JsValue> {
    let result = js_sys::Object::new();
    let files = js_sys::Array::new();
    for entry in package.files {
        let file = js_sys::Object::new();
        js_sys::Reflect::set(&file, &"path".into(), &entry.path.into())?;
        js_sys::Reflect::set(
            &file,
            &"bytes".into(),
            &js_sys::Uint8Array::from(entry.bytes.as_slice()),
        )?;
        files.push(&file);
    }
    let messages = js_sys::Array::new();
    for message in package.messages {
        messages.push(&message.into());
    }
    js_sys::Reflect::set(&result, &"files".into(), &files)?;
    js_sys::Reflect::set(
        &result,
        &"zip".into(),
        &js_sys::Uint8Array::from(package.zip.as_slice()),
    )?;
    js_sys::Reflect::set(&result, &"messages".into(), &messages)?;
    Ok(result.into())
}

pub(crate) async fn assemble(
    input: &str,
    read_file: js_sys::Function,
    expand_assets: js_sys::Function,
    read_asset: js_sys::Function,
    encode: js_sys::Function,
    validate_manifest: js_sys::Function,
) -> Result<JsValue, JsValue> {
    let plan = package_plan(input).map_err(error)?;
    let manifest =
        serde_json::to_string_pretty(&plan.manifest).map_err(|e| error(e.to_string()))?;
    let accepted = validate_manifest.call1(&JsValue::UNDEFINED, &JsValue::from_str(&manifest))?;
    if accepted.as_bool() != Some(true) {
        return Err(error(format!("Manifest is not valid:{manifest}")));
    }
    let mut sources = PackageSources::new(plan.copies).map_err(error)?;
    for asset in plan.assets {
        for expanded in expanded_assets(invoke(&expand_assets, &asset.pattern).await?)? {
            sources.expand(&asset, expanded).map_err(error)?;
        }
    }
    let content = String::from_utf8(read_bytes(&read_file, "README.md").await?)
        .map_err(|e| error(format!("Invalid README.md: {e}")))?;
    let readme = crate::bundle_readme_result(content, read_asset, encode).await?;
    let mut files = package_files(manifest, &readme);
    read_files(&read_file, sources, &mut files).await?;
    into_js(prepare(files, readme).map_err(error)?)
}
