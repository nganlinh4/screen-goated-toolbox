use super::*;

#[test]
fn saves_valid_svg_edits_and_rejects_active_content() {
    let path = std::env::temp_dir().join(format!(
        "sgt-svg-edit-{}-{}.svg",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>").unwrap();
    let edited =
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><path fill=\"#123456\" d=\"M0 0h1v1z\"/></svg>";
    assert!(write_svg_edits(&path, edited).is_ok());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
    assert!(write_svg_edits(&path, "<svg><script/></svg>").is_err());
    let over_bytes = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><text>{}</text></svg>",
        " ".repeat(MAX_EDIT_SOURCE_BYTES)
    );
    assert!(write_svg_edits(&path, &over_bytes).is_err());
    let over_geometry = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\">{}</svg>",
        "<path/>".repeat(MAX_EDITABLE_GEOMETRY + 1)
    );
    assert!(write_svg_edits(&path, &over_geometry).is_err());
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><a href=\"https://example.com\"><path/></a></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path fill=\"url(https://example.com/a.svg#x)\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path fill=\"url(#paint0)\"/></svg>"
        )
        .is_ok()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path style=\"fill:u\\72l(https://example.com/a)\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>.x{fill:u\\72l(https://example.com/a)}</style><path class=\"x\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><style><![CDATA[.x{fill:u/**/rl(https://example.com/a)}]]></style><path class=\"x\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><animate attributeName=\"opacity\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><filter id=\"blur\"><feGaussianBlur stdDeviation=\"99999\"/></filter><path filter=\"url(#blur)\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>.x { filter : blur(99999px) }</style><path class=\"x\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>.x{background:image-set(\"https://example.com/x.png\" 1x)}</style></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xi=\"http://www.w3.org/2001/XInclude\"><xi:include href=\"payload.svg\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg("<svg xmlns=\"http://www.w3.org/2000/svg\"><handler/><listener/></svg>")
            .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"paint\"><stop offset=\"0\" stop-color=\"#fff\"/></linearGradient><clipPath id=\"clip\"><path d=\"M0 0h1v1z\"/></clipPath></defs><g clip-path=\"url(#clip)\"><text fill=\"url(#paint)\">Safe</text><use href=\"#shape\"/></g><path id=\"shape\" d=\"M0 0h1v1z\"/></svg>"
        )
        .is_ok()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><g xmlns:xlink=\"http://www.w3.org/1999/xlink\"><use xlink:href=\"#shape\"/></g><path id=\"shape\"/></svg>"
        )
        .is_ok()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path fill=\"u\\72l(https://example.com/a.svg#x)\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>.paint{fill:url(#paint)}</style><linearGradient id=\"paint\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><style>@keyframes pulse{to{opacity:0}}.x{animation:pulse 1ms infinite}</style><path class=\"x\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><path style=\"transition:opacity 1ms\"/></svg>"
        )
        .is_err()
    );
    assert!(
        validate_svg(
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><image href=\"data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==\"/></svg>"
        )
        .is_err()
    );
    for hostile in [
        "<svg><path/></svg>",
        "<svg xmlns=\"urn:not-svg\"><path/></svg>",
        "<svg xmlns=\"HTTP://WWW.W3.ORG/2000/SVG\"><path/></svg>",
        "<svg XMLNS=\"http://www.w3.org/2000/svg\"><path/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:other=\"urn:not-svg\"><path/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:other=\"urn:test\"><path other:value=\"x\"/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:XLINK=\"http://www.w3.org/1999/xlink\"><use XLINK:href=\"#x\"/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><use xlink:href=\"#x\"/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><path foo:bar=\"x\"/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><use href=\"data:image/png;base64,AA==\"/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><a href=\"data:image/png;base64,AA==\"/></svg>",
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><path fill=\"url(data:image/png;base64,AA==)\"/></svg>",
    ] {
        assert!(validate_svg(hostile).is_err(), "{hostile}");
    }
    let mut definitions = String::new();
    for index in 0..16 {
        definitions.push_str(&format!(
            "<pattern id=\"n{index}\"><rect fill=\"url(#n{})\"/><rect mask=\"url(#n{})\"/></pattern>",
            index + 1,
            index + 1
        ));
    }
    let expansion_bomb = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><defs>{definitions}<pattern id=\"n16\"><path d=\"M0 0h1\"/></pattern></defs><rect fill=\"url(#n0)\"/></svg>"
    );
    assert!(validate_svg(&expansion_bomb).is_err());
    assert!(validate_svg("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg><svg></svg>").is_err());
    assert!(validate_svg("<svg xmlns=\"http://www.w3.org/2000/svg\">").is_err());
    let mut pixels = 0;
    add_embedded_raster_pixels(&mut pixels, 16_000_000).unwrap();
    add_embedded_raster_pixels(&mut pixels, 16_000_000).unwrap();
    assert!(add_embedded_raster_pixels(&mut pixels, 1).is_err());
    assert!(png_is_animated(
        b"\x89PNG\r\n\x1a\n\0\0\0\x08acTL\0\0\0\x01\0\0\0\0\0\0\0\0"
    ));
    let _ = std::fs::remove_file(path);
}

#[cfg(windows)]
#[test]
fn generated_svg_rejects_a_same_directory_symlink() {
    use std::os::windows::fs::symlink_file;

    let root = std::env::temp_dir().join(format!(
        "sgt-svg-same-directory-link-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let target = root.join("target.svg");
    let linked = root.join("result.svg");
    std::fs::write(
        &target,
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0h1v1z\"/></svg>",
    )
    .unwrap();
    if symlink_file(&target, &linked).is_err() {
        std::fs::remove_dir_all(root).unwrap();
        return;
    }
    assert!(
        validate_generated_result(json!({ "outputPath": linked }), &root, "assigned.svg").is_err()
    );
    std::fs::remove_file(linked).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
