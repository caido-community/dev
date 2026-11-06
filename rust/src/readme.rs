//! README URL transformation using Comrak and html5ever, with native image bundling.
use std::collections::HashMap;

use comrak::{format_commonmark, nodes::NodeValue, parse_document, Arena, Options};

mod html;
mod url_policy;

pub use url_policy::local_path;
use url_policy::{Context, Work};

const MAX_README_BYTES: usize = 2 * 1024 * 1024;

#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct ReadmeResult {
    pub(crate) content: String,
    pub(crate) warnings: Vec<String>,
    #[serde(default)]
    pub(crate) messages: Vec<String>,
}

fn process<'a>(
    content: &str,
    replacements: Option<&'a HashMap<String, String>>,
) -> Result<(String, Work<'a>), String> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.tagfilter = true;
    let root = parse_document(&arena, content, &options);
    let mut work = Work::new(replacements);
    // Reference definitions are resolved by the parser into ordinary links/images.
    // Each use is classified in its own context; unused definitions disappear.
    for node in root.descendants() {
        let mut data = node.data.borrow_mut();
        match &mut data.value {
            NodeValue::Image(image) => image.url = work.url(&image.url, Context::Image, "image")?,
            NodeValue::Link(link) => link.url = work.url(&link.url, Context::Link, "link")?,
            NodeValue::HtmlInline(value) => *value = html::transform(value, &mut work)?,
            NodeValue::HtmlBlock(block) => {
                block.literal = html::transform(&block.literal, &mut work)?;
            }
            _ => {}
        }
    }
    let output = if replacements.is_some() {
        let mut bytes = Vec::new();
        format_commonmark(root, &options, &mut bytes).map_err(|e| e.to_string())?;
        let output = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        if output.len() > MAX_README_BYTES {
            return Err(format!("README.md is {} bytes after inlining images, which exceeds the {MAX_README_BYTES} byte limit", output.len()));
        }
        output
    } else {
        String::new()
    };
    Ok((output, work))
}

pub(crate) fn asset_urls(content: &str) -> Result<Vec<String>, String> {
    let (_, work) = process(content, None)?;
    Ok(work.assets)
}

pub(crate) fn transform_content(
    content: &str,
    replacements: &HashMap<String, String>,
) -> Result<ReadmeResult, String> {
    let (content, work) = process(content, Some(replacements))?;
    Ok(ReadmeResult {
        content,
        warnings: work.warnings,
        messages: Vec::new(),
    })
}

pub(crate) fn bundle(root: &std::path::Path) -> Result<ReadmeResult, String> {
    let source = crate::filesystem::checked_source(root, std::path::Path::new("README.md"), false)
        .map_err(|error| format!("README.md is required in project root: {error}"))?;
    let content = std::fs::read_to_string(source)
        .map_err(|error| format!("Unable to read README.md: {error}"))?;
    let mut replacements = HashMap::new();
    let mut messages = Vec::new();
    for url in asset_urls(&content)? {
        let input = crate::filesystem::read_asset(root, &local_path(&url), &url)?;
        let converted = crate::readme_image::convert(&input, &url)?;
        replacements.insert(url, converted.url);
        messages.push(converted.message);
    }
    let mut result = transform_content(&content, &replacements)?;
    result.messages = messages;
    Ok(result)
}

#[cfg(test)]
#[path = "readme/tests.rs"]
mod tests;
