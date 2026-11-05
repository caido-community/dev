//! README URL transformation. Filesystem access and image encoding belong to the host.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use comrak::{format_commonmark, nodes::NodeValue, parse_document, Arena, Options};
use fluent_uri::{component::Scheme, IriRef};
use html5ever::serialize::Serializer;
use html5ever::{
    buffer_queue::BufferQueue,
    serialize::{HtmlSerializer, SerializeOpts},
    tendril::StrTendril,
    tokenizer::{TagKind, Token, TokenSink, TokenSinkResult, Tokenizer},
    QualName,
};
use typed_path::{Utf8UnixPath, Utf8WindowsComponent, Utf8WindowsPath};

const MAX_URI_BYTES: usize =
    crate::readme_image::MAX_IMAGE_BYTES.div_ceil(3) * 4 + "data:image/webp;base64,".len();
const MAX_README_BYTES: usize = 2 * 1024 * 1024;

#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct ReadmeResult {
    pub(crate) content: String,
    pub(crate) warnings: Vec<String>,
    #[serde(default)]
    pub(crate) messages: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Context {
    Image,
    Link,
    Resource,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Empty,
    Fragment,
    DataImage,
    LocalImage,
    LocalOther,
    Blocked,
}

/// Decode the pathname only, matching decodeURIComponent's all-or-nothing behavior.
/// The host must still check containment and symlinks before opening this path.
pub fn local_path(url: &str) -> String {
    // Raw HTML permits spaces in local filenames. Escape only for parsing, without
    // resolving against a base URL or normalizing filesystem path segments.
    let escaped = url
        .replace(' ', "%20")
        .replace('[', "%5B")
        .replace(']', "%5D");
    let Ok(reference) = IriRef::parse(escaped.as_str()) else {
        return url.to_owned();
    };
    let pathname = reference.path().as_str();
    let mut decoded = Vec::with_capacity(pathname.len());
    let mut bytes = pathname.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
            let Some(high) = bytes.next().and_then(hex) else {
                return pathname.to_owned();
            };
            let Some(low) = bytes.next().and_then(hex) else {
                return pathname.to_owned();
            };
            decoded.push(high * 16 + low);
        } else {
            decoded.push(byte);
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| pathname.to_owned())
}
fn data_image(url: &str) -> bool {
    if url.len() > MAX_URI_BYTES {
        return false;
    }
    let Some((header, _)) = url.split_once(',') else {
        return false;
    };
    // data-url keeps its encoding flag private. This is a base64-only policy
    // check, not MIME parsing; the parser below handles MIME and body decoding.
    if !header
        .rsplit(';')
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case("base64"))
    {
        return false;
    }
    let Ok(data) = data_url::DataUrl::process(url) else {
        return false;
    };
    let mime = data.mime_type();
    if !mime.type_.eq_ignore_ascii_case("image") {
        return false;
    }
    // Use the parsed MIME essence; parameters are not part of the image format.
    let essence = format!("{}/{}", mime.type_, mime.subtype).to_ascii_lowercase();
    let format = image::ImageFormat::from_mime_type(&essence).or_else(|| {
        // Preserve the existing nonstandard JPEG alias.
        mime.matches("image", "jpg")
            .then_some(image::ImageFormat::Jpeg)
    });
    let Some(format) = format else {
        return false;
    };
    let mut bytes = Vec::new();
    let decoded = data.decode(|chunk| {
        if bytes.len() + chunk.len() > crate::readme_image::MAX_IMAGE_BYTES {
            return Err(());
        }
        bytes.extend_from_slice(chunk);
        Ok(())
    });
    decoded.is_ok() && crate::readme_image::validate(&bytes, format).is_ok()
}
fn classify(url: &str) -> Kind {
    if url.is_empty() {
        return Kind::Empty;
    }
    let escaped = url
        .replace(' ', "%20")
        .replace('[', "%5B")
        .replace(']', "%5D");
    let Ok(reference) = IriRef::parse(escaped.as_str()) else {
        return Kind::Blocked;
    };
    if let Some(scheme) = reference.scheme() {
        return if scheme == Scheme::new_or_panic("data") {
            Kind::DataImage
        } else {
            Kind::Blocked
        };
    }
    if reference.authority().is_some() {
        return Kind::Blocked;
    }
    if reference.path().is_empty() && reference.query().is_none() && reference.fragment().is_some()
    {
        return Kind::Fragment;
    }
    let path = local_path(url);
    let windows = Utf8WindowsPath::new(&path);
    if Utf8UnixPath::new(&path).has_root()
        || windows.has_root()
        || matches!(
            windows.components().next(),
            Some(Utf8WindowsComponent::Prefix(_))
        )
    {
        return Kind::Blocked;
    }
    let extension = windows.extension().map(str::to_ascii_lowercase);
    if extension.is_some_and(|ext| {
        matches!(
            ext.as_str(),
            "avif" | "gif" | "jpeg" | "jpg" | "png" | "svg" | "tif" | "tiff" | "webp"
        )
    }) {
        Kind::LocalImage
    } else {
        Kind::LocalOther
    }
}
fn preserve(kind: Kind, context: Context) -> bool {
    match kind {
        Kind::Empty => true,
        Kind::Fragment | Kind::LocalOther => context == Context::Link,
        Kind::DataImage => context != Context::Link,
        _ => false,
    }
}

struct Work<'a> {
    replacements: Option<&'a HashMap<String, String>>,
    assets: Vec<String>,
    seen: HashSet<String>,
    warnings: Vec<String>,
    data_images: HashMap<String, bool>,
}
impl Work<'_> {
    fn data_image(&mut self, url: &str) -> bool {
        *self
            .data_images
            .entry(url.to_owned())
            .or_insert_with(|| data_image(url))
    }
    fn url(&mut self, original: &str, context: Context, label: &str) -> Result<String, String> {
        let url = original.trim();
        // Browsers remove embedded tab/newline controls while resolving schemes.
        // Reject them before classification, including entity-decoded HTML values.
        let kind = if original.bytes().any(|b| b.is_ascii_control()) {
            Kind::Blocked
        } else {
            classify(url)
        };
        if preserve(kind, context)
            && (kind != Kind::DataImage || self.replacements.is_none() || self.data_image(url))
        {
            return Ok(url.into());
        }
        if kind == Kind::LocalImage {
            if self.seen.insert(url.into()) {
                self.assets.push(url.into());
            }
            if let Some(replacements) = self.replacements {
                let replacement = replacements
                    .get(url)
                    .ok_or_else(|| format!("Missing replacement for README image: {url}"))?;
                if !self.data_image(replacement) {
                    return Err(format!(
                        "Invalid or oversized replacement for README image: {url}"
                    ));
                }
                return Ok(replacement.clone());
            }
            return Ok(url.into());
        }
        let display: String = url.chars().take(200).collect();
        let suffix = if url.chars().count() > 200 { "..." } else { "" };
        self.warnings.push(format!(
            "Warning: Skipping unsupported {label} URL in README: {display}{suffix}"
        ));
        Ok(String::new())
    }
}

#[derive(Default)]
struct HtmlTokens(RefCell<Vec<Token>>);
impl TokenSink for HtmlTokens {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        self.0.borrow_mut().push(token);
        TokenSinkResult::Continue
    }
}

/// Serialize tokens, not a DOM: inline opening/closing tags are separate Markdown
/// nodes, and constructing a tree would insert closing tags at those boundaries.
fn html(value: &str, work: &mut Work<'_>) -> Result<String, String> {
    let tokenizer = Tokenizer::new(HtmlTokens::default(), Default::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(value));
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let mut output = Vec::new();
    for token in tokenizer.sink.0.into_inner() {
        // A fresh serializer avoids its DOM stack suppressing subsequent tokens
        // after void elements, or requiring matching tags within this fragment.
        let mut writer = HtmlSerializer::new(
            &mut output,
            SerializeOpts {
                create_missing_parent: true,
                ..Default::default()
            },
        );
        let result = match token {
            Token::TagToken(mut tag) => {
                let name = QualName::new(None, html5ever::ns!(html), tag.name);
                if tag.kind == TagKind::EndTag {
                    writer.end_elem(name)
                } else {
                    for attr in &mut tag.attrs {
                        let key = attr.name.local.as_ref();
                        let value = if matches!(key, "style" | "srcset") {
                            String::new()
                        } else if matches!(
                            key,
                            "action"
                                | "background"
                                | "data"
                                | "formaction"
                                | "href"
                                | "poster"
                                | "src"
                                | "xlink:href"
                        ) {
                            let context = if key == "href" {
                                Context::Link
                            } else {
                                Context::Resource
                            };
                            work.url(
                                &attr.value,
                                context,
                                if context == Context::Link {
                                    "HTML link"
                                } else {
                                    "HTML resource"
                                },
                            )?
                        } else {
                            attr.value.to_string()
                        };
                        attr.value = value.into();
                    }
                    writer.start_elem(
                        name,
                        tag.attrs
                            .iter()
                            .map(|attr| (&attr.name, attr.value.as_ref())),
                    )
                }
            }
            Token::CharacterTokens(text) => writer.write_text(&text),
            Token::NullCharacterToken => writer.write_text("\u{fffd}"),
            Token::CommentToken(text) => writer.write_comment(&text),
            Token::DoctypeToken(doctype) => {
                writer.write_doctype(doctype.name.as_deref().unwrap_or("html"))
            }
            Token::ParseError(_) | Token::EOFToken => Ok(()),
        };
        result.map_err(|e| format!("Unable to serialize README HTML: {e}"))?;
    }
    String::from_utf8(output).map_err(|e| e.to_string())
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
    let mut work = Work {
        replacements,
        assets: Vec::new(),
        seen: HashSet::new(),
        warnings: Vec::new(),
        data_images: HashMap::new(),
    };
    // Reference definitions are resolved by the parser into ordinary links/images.
    // Each use is classified in its own context; unused definitions disappear.
    for node in root.descendants() {
        let mut data = node.data.borrow_mut();
        match &mut data.value {
            NodeValue::Image(image) => image.url = work.url(&image.url, Context::Image, "image")?,
            NodeValue::Link(link) => link.url = work.url(&link.url, Context::Link, "link")?,
            NodeValue::HtmlInline(value) => *value = html(value, &mut work)?,
            NodeValue::HtmlBlock(block) => block.literal = html(&block.literal, &mut work)?,
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
mod tests {
    use super::*;

    #[test]
    fn native_path_decoding_preserves_utf8_and_invalid_sequences() {
        assert_eq!(
            local_path("caf%C3%A9%20image.png?x=1#preview"),
            "café image.png"
        );
        assert_eq!(local_path("bad%FF.png"), "bad%FF.png");
        assert_eq!(local_path("a+b.png"), "a+b.png");
    }

    #[test]
    fn rewrites_external_urls_and_preserves_links_and_fragments() {
        let result = transform_content("[external](https://example.com) [local](docs/page.md) [anchor](#purpose)\n\n<img src=\"https://example.com/a.png\">", &HashMap::new()).unwrap();
        assert!(!result.content.contains("https://example.com"));
        assert!(result.content.contains("[local](docs/page.md)"));
        assert!(result.content.contains("[anchor](#purpose)"));
        assert!(result.content.contains("src=\"\""));
        assert_eq!(result.warnings.len(), 2);
    }

    #[test]
    fn blocks_encoded_absolute_paths_and_unsafe_schemes() {
        let result = transform_content(
            "[bad](javascript:alert) ![bad](%2Fetc%2Fimage.png)\n",
            &HashMap::new(),
        )
        .unwrap();
        assert!(!result.content.contains("javascript:"));
        assert!(!result.content.contains("%2Fetc"));
        assert_eq!(result.warnings.len(), 2);
    }

    fn png_uri() -> String {
        use base64::Engine;
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(include_bytes!(
                "../../src/bundle/__fixtures__/images/red16x16.png"
            ))
        )
    }

    #[test]
    fn data_images_are_validated_in_each_use_context() {
        let png = png_uri();
        for (uri, preserved) in [
            (png.clone(), true),
            (png.replace("image/png", "image/jpeg"), false),
            (png.replace("image/png", "image/svg+xml"), false),
            (png.replace("image/png", "image/avif"), false),
            ("data:image/png,%89PNG".into(), false),
            ("data:image/png;base64,broken".into(), false),
            (
                png.replace("image/png;base64", "IMAGE/PNG;charset=utf-8;BASE64"),
                true,
            ),
        ] {
            let source = format!("![Image]({uri})\n\n![Repeated]({uri})\n\n[Link]({uri})\n\n<img src=\"{uri}\">\n\n<a href=\"{uri}\">Link</a>");
            let result = transform_content(&source, &HashMap::new()).unwrap();
            let image = if preserved {
                format!("![Image]({uri})")
            } else {
                "![Image]()".into()
            };
            let repeated = if preserved {
                format!("![Repeated]({uri})")
            } else {
                "![Repeated]()".into()
            };
            let resource = if preserved {
                format!("src=\"{uri}\"")
            } else {
                "src=\"\"".into()
            };
            assert!(result.content.contains(&image), "{}", result.content);
            assert!(result.content.contains(&repeated));
            assert!(result.content.contains(&resource));
            assert!(result.content.contains("[Link]()"));
            assert!(result.content.contains("href=\"\""));
            assert_eq!(result.warnings.len(), if preserved { 2 } else { 5 });
        }
    }

    #[test]
    fn reference_definitions_are_classified_per_use() {
        let uri = png_uri();
        let result = transform_content(&format!("![Image][shared] and [Link][shared]\n\n[shared]: {uri}\n\n[unused]: https://example.com"), &HashMap::new()).unwrap();
        assert!(result.content.contains(&format!("![Image]({uri})")));
        assert!(result.content.contains("[Link]()"));
        assert!(!result.content.contains("[shared]:"));
        assert!(!result.content.contains("[unused]:"));
    }

    #[test]
    fn blocks_all_unsupported_and_malformed_url_forms() {
        for url in [
            "https://example.com",
            "//example.com/a",
            "javascript:alert(1)",
            "mailto:a@example.com",
            "file:///tmp/a",
            "blob:https://example.com/id",
            "custom+scheme.value:asset.png",
            "%2Foutside.png",
            "C%3A%2Foutside.png",
            "bad%ZZ.png",
            "bad\\path.png",
        ] {
            let result = transform_content(
                &format!("<img src=\"{url}\">\n\n<a href=\"{url}\">link</a>"),
                &HashMap::new(),
            )
            .unwrap();
            assert!(
                result.content.contains("src=\"\""),
                "{url}: {}",
                result.content
            );
            assert!(
                result.content.contains("href=\"\""),
                "{url}: {}",
                result.content
            );
            assert_eq!(result.warnings.len(), 2, "{url}");
        }
    }

    #[test]
    fn html_parser_preserves_inline_boundaries_and_clears_resource_attributes() {
        let uri = png_uri();
        let replacements = HashMap::from([("local.png".into(), uri.clone())]);
        let source = "<IMG SRC=local.png ALT='Local image'>\n\n<a HREF=https://example.com>external</a>\n\nbefore <a href=\"#foo\">click</a> after\n\nbefore <a href=\"java&#x09;script:alert(1)\">unsafe</a> after\n\n<video poster=\"https://example.com\"></video><object data=\"https://example.com\"></object><form action=\"https://example.com\"></form><button formaction=\"https://example.com\">Send</button><table background=\"https://example.com\"></table><img srcset=\"local.png 1x\" src=\"local.png\"><a style=\"background:url(https://example.com)\" href=\"assets/test.txt\">Local</a>";
        let result = transform_content(source, &replacements).unwrap();
        assert!(result.content.contains(&format!("src=\"{uri}\"")));
        assert!(result
            .content
            .contains("before <a href=\"#foo\">click</a> after"));
        assert!(result
            .content
            .contains("before <a href=\"\">unsafe</a> after"));
        for attribute in [
            "poster",
            "data",
            "action",
            "formaction",
            "background",
            "srcset",
            "style",
        ] {
            assert!(
                result.content.contains(&format!("{attribute}=\"\"")),
                "{attribute}: {}",
                result.content
            );
        }
        assert!(!result.content.contains("https://example.com"));
        assert!(result.content.contains("href=\"assets/test.txt\""));
    }

    #[test]
    fn data_image_byte_limit_is_inclusive() {
        use base64::Engine;
        let mut bytes =
            include_bytes!("../../src/bundle/__fixtures__/images/red16x16.png").to_vec();
        bytes.resize(crate::readme_image::MAX_IMAGE_BYTES, 0);
        let uri = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        assert!(data_image(&uri));
        bytes.push(0);
        let oversized = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        let result =
            transform_content(&format!("![oversized]({oversized})"), &HashMap::new()).unwrap();
        assert!(result.content.contains("![oversized]()"));
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn bundles_local_paths_and_rejects_missing_unsupported_or_escaping_images() {
        let root = crate::filesystem::tests::TempDir::new();
        let image = include_bytes!("../../src/bundle/__fixtures__/images/red16x16.png");
        std::fs::create_dir(root.0.join("asset dir")).unwrap();
        for name in [
            "asset dir/test image.png",
            "café image.PNG",
            "figure[1].png",
        ] {
            std::fs::write(root.0.join(name), image).unwrap();
        }
        for source in [
            "![image](asset%20dir/test%20image.png?raw=true#preview)",
            "<img src=\"café image.PNG?version=1#preview\">",
            "![image](figure[1].png)",
            "<img src=\"figure[1].png\">",
        ] {
            std::fs::write(root.0.join("README.md"), source).unwrap();
            let result = bundle(&root.0).unwrap();
            assert!(result.content.contains("data:image/webp;base64,"));
            assert_eq!(result.messages.len(), 1);
        }
        std::fs::write(root.0.join("image.svg"), "<svg></svg>").unwrap();
        for source in [
            "![image](missing.png)",
            "![image](image.svg)",
            "![image](../outside.png)",
        ] {
            std::fs::write(root.0.join("README.md"), source).unwrap();
            assert!(bundle(&root.0).is_err(), "{source}");
        }
        std::fs::remove_file(root.0.join("README.md")).unwrap();
        assert!(bundle(&root.0)
            .err()
            .unwrap()
            .contains("README.md is required"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_leaf_and_parent_image_symlinks() {
        let root = crate::filesystem::tests::TempDir::new();
        let outside = crate::filesystem::tests::TempDir::new();
        std::fs::write(
            outside.0.join("image.png"),
            include_bytes!("../../src/bundle/__fixtures__/images/red16x16.png"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.0.join("image.png"), root.0.join("link.png")).unwrap();
        std::os::unix::fs::symlink(&outside.0, root.0.join("linked")).unwrap();
        for source in ["![image](link.png)", "![image](linked/image.png)"] {
            std::fs::write(root.0.join("README.md"), source).unwrap();
            assert!(bundle(&root.0).is_err(), "{source}");
        }
    }

    #[test]
    fn enforces_final_readme_limit() {
        assert!(transform_content(&"a".repeat(MAX_README_BYTES + 1), &HashMap::new()).is_err());
    }
}
