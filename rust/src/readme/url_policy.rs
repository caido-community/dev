use std::collections::{HashMap, HashSet};

use fluent_uri::{component::Scheme, IriRef};
use typed_path::{Utf8UnixPath, Utf8WindowsComponent, Utf8WindowsPath};

const MAX_URI_BYTES: usize =
    crate::readme_image::MAX_IMAGE_BYTES.div_ceil(3) * 4 + "data:image/webp;base64,".len();

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Context {
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
pub(super) fn data_image(url: &str) -> bool {
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

pub(super) struct Work<'a> {
    replacements: Option<&'a HashMap<String, String>>,
    pub(super) assets: Vec<String>,
    seen: HashSet<String>,
    pub(super) warnings: Vec<String>,
    data_images: HashMap<String, bool>,
}
impl<'a> Work<'a> {
    pub(super) fn new(replacements: Option<&'a HashMap<String, String>>) -> Self {
        Self {
            replacements,
            assets: Vec::new(),
            seen: HashSet::new(),
            warnings: Vec::new(),
            data_images: HashMap::new(),
        }
    }
    fn data_image(&mut self, url: &str) -> bool {
        *self
            .data_images
            .entry(url.to_owned())
            .or_insert_with(|| data_image(url))
    }
    pub(super) fn url(
        &mut self,
        original: &str,
        context: Context,
        label: &str,
    ) -> Result<String, String> {
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
