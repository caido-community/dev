use base64::{engine::general_purpose::STANDARD, Engine};
use image::{
    imageops::FilterType, AnimationDecoder, ImageDecoder, ImageFormat, ImageReader, Limits,
};
use std::io::Cursor;
use webp::{Encoder, WebPConfig};

pub(crate) const MAX_IMAGE_BYTES: usize = 100 * 1024;
const MAX_INPUT_PIXELS: u64 = 40_000_000;
const MAX_DIMENSION: u32 = 1600;

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_INPUT_PIXELS as u32);
    limits.max_image_height = Some(MAX_INPUT_PIXELS as u32);
    limits.max_alloc = Some(MAX_INPUT_PIXELS * 8);
    limits
}

fn reader(input: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, String> {
    let mut reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Gif | ImageFormat::WebP)
    ) {
        return Err("Supported README image formats are PNG, JPEG, GIF, and WebP".into());
    }
    let (width, height) = ImageReader::with_format(Cursor::new(input), reader.format().unwrap())
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_INPUT_PIXELS {
        return Err(format!(
            "README image exceeds the {MAX_INPUT_PIXELS} input pixel limit"
        ));
    }
    reader.limits(limits());
    Ok(reader)
}

fn validate_gif(input: &[u8]) -> Result<(), String> {
    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(input))
        .map_err(|error| error.to_string())?;
    decoder
        .set_limits(limits())
        .map_err(|error| error.to_string())?;
    // Consume one frame at a time rather than collecting an animation in memory.
    let mut has_frame = false;
    for frame in decoder.into_frames() {
        frame.map_err(|error| error.to_string())?;
        has_frame = true;
    }
    if !has_frame {
        return Err("README GIF contains no frames".into());
    }
    Ok(())
}

pub(crate) fn validate(input: &[u8], format: ImageFormat) -> Result<(), String> {
    if input.is_empty()
        || input.len() > MAX_IMAGE_BYTES
        || image::guess_format(input).ok() != Some(format)
    {
        return Err("Invalid, mismatched, or oversized README image".into());
    }
    let reader = reader(input)?;
    match format {
        ImageFormat::Gif => validate_gif(input),
        ImageFormat::WebP => {
            let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(input))
                .map_err(|error| error.to_string())?;
            decoder
                .set_limits(limits())
                .map_err(|error| error.to_string())?;
            if decoder.has_animation() {
                let mut has_frame = false;
                for frame in decoder.into_frames() {
                    frame.map_err(|error| error.to_string())?;
                    has_frame = true;
                }
                if !has_frame {
                    return Err("README WebP animation contains no frames".into());
                }
                Ok(())
            } else {
                reader
                    .decode()
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
        }
        _ => reader
            .decode()
            .map(|_| ())
            .map_err(|error| error.to_string()),
    }
}

pub(crate) fn preserve_gif(input: &[u8]) -> Result<bool, String> {
    if input.len() > 100 * 1024 || image::guess_format(input).ok() != Some(ImageFormat::Gif) {
        return Ok(false);
    }
    let (width, height) = ImageReader::with_format(Cursor::new(input), ImageFormat::Gif)
        .into_dimensions()
        .map_err(|error| format!("Unable to process README GIF: {error}"))?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Ok(false);
    }
    validate_gif(input).map_err(|error| format!("Unable to process README GIF: {error}"))?;
    Ok(true)
}

fn prepare(input: &[u8], url: &str) -> Result<image::RgbaImage, String> {
    let image = reader(input)
        .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
        .map_err(|error| format!("Unable to process README image {url}: {error}"))?;
    let image = if image.width() > MAX_DIMENSION || image.height() > MAX_DIMENSION {
        image.resize(MAX_DIMENSION, MAX_DIMENSION, FilterType::Lanczos3)
    } else {
        image
    }
    .to_rgba8();
    Ok(image)
}

pub(crate) struct ConvertedImage {
    pub url: String,
    pub message: String,
}

fn result(bytes: &[u8], format: &str, url: &str) -> ConvertedImage {
    let label = if format == "gif" { "GIF" } else { "WebP" };
    ConvertedImage {
        url: format!("data:image/{format};base64,{}", STANDARD.encode(bytes)),
        message: format!(
            "Inlined README image as {label}: {url} ({} bytes)",
            bytes.len()
        ),
    }
}

#[cfg(test)]
#[path = "readme_image/tests.rs"]
mod tests;

/// Decode, resize, encode, and enforce the packaging limits natively.
pub(crate) fn convert(input: &[u8], url: &str) -> Result<ConvertedImage, String> {
    if preserve_gif(input)? {
        return Ok(result(input, "gif", url));
    }
    let prepared = prepare(input, url)?;
    let encoder = Encoder::from_rgba(prepared.as_raw(), prepared.width(), prepared.height());
    let mut output_size = 0;
    for quality in (20..=80).rev().step_by(10) {
        let mut config = WebPConfig::new()
            .map_err(|_| format!("Unable to initialize WebP encoder for README image {url}"))?;
        config.quality = quality as f32;
        config.method = 6;
        config.lossless = 0;
        let output = encoder
            .encode_advanced(&config)
            .map_err(|error| format!("Unable to encode README image {url}: {error:?}"))?;
        let bytes: &[u8] = &output;
        output_size = bytes.len();
        if output_size <= MAX_IMAGE_BYTES {
            return Ok(result(bytes, "webp", url));
        }
    }
    Err(format!(
        "README image {url} is {output_size} bytes after compression, which exceeds the {MAX_IMAGE_BYTES} byte limit"
    ))
}
