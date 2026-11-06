//! README URL transformation using Comrak and html5ever, with native image bundling.
use std::path::Path;

use comrak::{format_commonmark, nodes::NodeValue, parse_document, Arena, Options};

mod html;
mod url_policy;

use url_policy::{Context, Work};

const MAX_README_BYTES: usize = 2 * 1024 * 1024;

pub(crate) struct ReadmeResult {
    pub(crate) content: String,
    pub(crate) warnings: Vec<String>,
    pub(crate) messages: Vec<String>,
}

fn transform_content(content: &str, project: &Path) -> Result<ReadmeResult, String> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.tagfilter = true;
    let root = parse_document(&arena, content, &options);
    let mut work = Work::new(project);
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
    let mut bytes = Vec::new();
    format_commonmark(root, &options, &mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_README_BYTES {
        return Err(format!("README.md is {} bytes after inlining images, which exceeds the {MAX_README_BYTES} byte limit", bytes.len()));
    }
    Ok(ReadmeResult {
        content: String::from_utf8(bytes).map_err(|e| e.to_string())?,
        warnings: work.warnings,
        messages: work.messages,
    })
}

pub(crate) fn bundle(root: &Path) -> Result<ReadmeResult, String> {
    let source = crate::filesystem::checked_source(root, Path::new("README.md"), false)
        .map_err(|error| format!("README.md is required in project root: {error}"))?;
    let content = std::fs::read_to_string(source)
        .map_err(|error| format!("Unable to read README.md: {error}"))?;
    transform_content(&content, root)
}

#[cfg(test)]
#[path = "readme/tests.rs"]
mod tests;
