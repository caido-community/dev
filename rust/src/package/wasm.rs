use super::{
    package_files, package_plan, prepare, ExpandedAsset, PackageFile, PackageSources,
    PreparedPackage, ReadmeResult,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use wasm_bindgen::{prelude::*, JsCast};

#[derive(Deserialize)]
struct SourceFile {
    source: String,
    #[serde(deserialize_with = "file_bytes")]
    bytes: Vec<u8>,
}

fn file_bytes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let value: JsValue = serde_wasm_bindgen::preserve::deserialize(deserializer)?;
    if !value.is_instance_of::<js_sys::Uint8Array>() {
        return Err(serde::de::Error::custom(
            "Expected file bytes as Uint8Array",
        ));
    }
    Ok(js_sys::Uint8Array::new(&value).to_vec())
}

#[derive(Deserialize)]
struct AssetExpansion {
    pattern: String,
    files: Vec<ExpandedAsset>,
}

#[derive(Deserialize)]
struct PackageInputs {
    files: Vec<SourceFile>,
    assets: Vec<AssetExpansion>,
    readme: ReadmeResult,
}

#[derive(Serialize)]
struct PackageFileOutput<'a> {
    path: &'a str,
    #[serde(with = "serde_bytes")]
    bytes: &'a [u8],
}

#[derive(Serialize)]
struct PackageOutput<'a> {
    files: Vec<PackageFileOutput<'a>>,
    #[serde(with = "serde_bytes")]
    zip: &'a [u8],
    messages: &'a [String],
}

fn error(message: impl AsRef<str>) -> JsValue {
    JsValue::from_str(message.as_ref())
}

fn into_js(package: PreparedPackage) -> Result<JsValue, JsValue> {
    let output = PackageOutput {
        files: package
            .files
            .iter()
            .map(|entry| PackageFileOutput {
                path: &entry.path,
                bytes: &entry.bytes,
            })
            .collect(),
        zip: &package.zip,
        messages: &package.messages,
    };
    serde_wasm_bindgen::to_value(&output).map_err(|e| error(format!("Invalid package output: {e}")))
}

pub(crate) fn assemble(input: JsValue, preloaded: JsValue) -> Result<JsValue, JsValue> {
    let input = serde_wasm_bindgen::from_value(input)
        .map_err(|e| error(format!("Invalid package input: {e}")))?;
    let plan = package_plan(input).map_err(error)?;
    let preloaded: PackageInputs = serde_wasm_bindgen::from_value(preloaded)
        .map_err(|e| error(format!("Invalid preloaded package inputs: {e}")))?;
    let mut contents = HashMap::new();
    for file in preloaded.files {
        if contents.insert(file.source.clone(), file.bytes).is_some() {
            return Err(error(format!(
                "Duplicate preloaded source '{}'",
                file.source
            )));
        }
    }
    let mut expansions = HashMap::new();
    for expansion in preloaded.assets {
        if expansions
            .insert(expansion.pattern.clone(), expansion.files)
            .is_some()
        {
            return Err(error(format!(
                "Duplicate preloaded asset pattern '{}'",
                expansion.pattern
            )));
        }
    }
    let manifest =
        serde_json::to_string_pretty(&plan.manifest).map_err(|e| error(e.to_string()))?;
    let mut sources = PackageSources::new(plan.copies).map_err(error)?;
    for asset in plan.assets {
        let expanded = expansions.get(&asset.pattern).ok_or_else(|| {
            error(format!(
                "Missing preloaded asset pattern '{}'",
                asset.pattern
            ))
        })?;
        for file in expanded {
            sources.expand(&asset, file.clone()).map_err(error)?;
        }
    }
    let mut files = package_files(manifest, &preloaded.readme);
    for copy in sources.copies {
        let bytes = contents
            .get(&copy.source)
            .ok_or_else(|| error(format!("Missing preloaded source '{}'", copy.source)))?;
        files.push(PackageFile {
            path: copy.destination,
            bytes: bytes.clone(),
        });
    }
    into_js(prepare(files, preloaded.readme).map_err(error)?)
}
