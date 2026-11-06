use super::url_policy::data_image;
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
            "../../../src/bundle/__fixtures__/images/red16x16.png"
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
    let mut bytes = include_bytes!("../../../src/bundle/__fixtures__/images/red16x16.png").to_vec();
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
    let result = transform_content(&format!("![oversized]({oversized})"), &HashMap::new()).unwrap();
    assert!(result.content.contains("![oversized]()"));
    assert_eq!(result.warnings.len(), 1);
}

#[test]
fn bundles_local_paths_and_rejects_missing_unsupported_or_escaping_images() {
    let root = crate::filesystem::tests::TempDir::new();
    let image = include_bytes!("../../../src/bundle/__fixtures__/images/red16x16.png");
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
        include_bytes!("../../../src/bundle/__fixtures__/images/red16x16.png"),
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
