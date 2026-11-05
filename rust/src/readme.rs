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
use serde_json::json;
use typed_path::{Utf8UnixPath, Utf8WindowsComponent, Utf8WindowsPath};

const MAX_URI_BYTES: usize = (100_usize * 1024 * 4).div_ceil(3) + "data:image/webp;base64,".len();
const MAX_README_BYTES: usize = 2 * 1024 * 1024;

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
    let escaped = url.replace(' ', "%20");
    let Ok(reference) = IriRef::parse(escaped.as_str()) else {
        return url.to_owned();
    };
    let pathname = reference.path().as_str();
    js_sys::decode_uri_component(pathname)
        .map(String::from)
        .unwrap_or_else(|_| pathname.to_owned())
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
    let escaped = url.replace(' ', "%20");
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

/// Return a JSON array of distinct local image URLs, in source traversal order.
pub fn assets(content: &str) -> Result<String, String> {
    let (_, work) = process(content, None)?;
    serde_json::to_string(&work.assets).map_err(|e| e.to_string())
}

/// Replace local images using a JSON object keyed by the original trimmed URL.
/// Return JSON `{content, warnings}`; warnings use the CLI's existing wording.
pub fn transform(content: &str, replacements_json: &str) -> Result<String, String> {
    let replacements: HashMap<String, String> = serde_json::from_str(replacements_json)
        .map_err(|e| format!("Invalid README replacements: {e}"))?;
    let (content, work) = process(content, Some(&replacements))?;
    Ok(json!({ "content": content, "warnings": work.warnings }).to_string())
}
