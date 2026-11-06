use super::*;

#[test]
fn native_encoder_converts_checked_in_fixture() {
    let input = include_bytes!("../../../playgrounds/build-frontend/assets/test.png");
    let converted = convert(input, "test.png").unwrap();
    let bytes = STANDARD
        .decode(
            converted
                .url
                .strip_prefix("data:image/webp;base64,")
                .unwrap(),
        )
        .unwrap();
    assert!(bytes.len() <= MAX_IMAGE_BYTES);
    assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::WebP);
    let original = image::load_from_memory(input).unwrap();
    let encoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(
        (encoded.width(), encoded.height()),
        (original.width(), original.height())
    );
}

#[test]
fn converts_without_enlarging_and_resizes_large_images() {
    for (input, dimensions) in [
        (
            include_bytes!("../../../src/bundle/__fixtures__/images/red16x16.png").as_slice(),
            (16, 16),
        ),
        (
            include_bytes!("../../../src/bundle/__fixtures__/images/red256x256.png").as_slice(),
            (256, 256),
        ),
        (
            include_bytes!("../../../src/bundle/__fixtures__/images/red3200x2400.png").as_slice(),
            (1600, 1200),
        ),
    ] {
        let converted = convert(input, "fixture.png").unwrap();
        let bytes = STANDARD
            .decode(
                converted
                    .url
                    .strip_prefix("data:image/webp;base64,")
                    .unwrap(),
            )
            .unwrap();
        assert!(bytes.len() <= MAX_IMAGE_BYTES);
        assert!(bytes.len() < input.len());
        let output = image::load_from_memory(&bytes).unwrap();
        assert_eq!((output.width(), output.height()), dimensions);
    }
}

#[test]
fn preserves_small_gif_bytes_and_converts_large_or_oversized_gifs() {
    let animated = include_bytes!("../../../src/bundle/__fixtures__/images/animated.gif");
    let converted = convert(animated, "animated.gif").unwrap();
    assert_eq!(
        converted.url,
        format!("data:image/gif;base64,{}", STANDARD.encode(animated))
    );
    let mut oversized = animated.to_vec();
    oversized.resize(MAX_IMAGE_BYTES + 1, 0);
    for input in [
        oversized.as_slice(),
        include_bytes!("../../../src/bundle/__fixtures__/images/large.gif").as_slice(),
    ] {
        let converted = convert(input, "large.gif").unwrap();
        let bytes = STANDARD
            .decode(
                converted
                    .url
                    .strip_prefix("data:image/webp;base64,")
                    .unwrap(),
            )
            .unwrap();
        assert!(bytes.len() <= MAX_IMAGE_BYTES);
        let output = image::load_from_memory(&bytes).unwrap();
        assert!(output.width() <= MAX_DIMENSION && output.height() <= MAX_DIMENSION);
    }
}

#[test]
fn validates_complete_animation_and_declared_format() {
    let gif = include_bytes!("../../../src/bundle/__fixtures__/images/animated.gif");
    assert!(validate(gif, ImageFormat::Gif).is_ok());
    assert!(validate(gif, ImageFormat::Png).is_err());
    assert!(validate(&gif[..gif.len() / 2], ImageFormat::Gif).is_err());
    assert!(validate(&[], ImageFormat::Png).is_err());
    assert!(validate(&vec![0; MAX_IMAGE_BYTES + 1], ImageFormat::Png).is_err());
}

#[test]
fn rejects_unsupported_input() {
    assert!(convert(b"not an image", "invalid.svg").is_err());
}
