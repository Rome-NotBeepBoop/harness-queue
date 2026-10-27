use harness_queue::image::{
    describe_attachments, extract_image_references, format_prompt_with_images, is_image_path,
    resolve_path,
};
use harness_queue::models::HarnessKind;
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[test]
fn test_is_image_path() {
    assert!(is_image_path(Path::new("photo.png")));
    assert!(is_image_path(Path::new("/var/folders/test.JPEG")));
    assert!(is_image_path(Path::new("test.webp")));
    assert!(is_image_path(Path::new("icon.svg")));
    assert!(!is_image_path(Path::new("file.txt")));
    assert!(!is_image_path(Path::new("code.rs")));
}

#[test]
fn test_resolve_path() {
    assert_eq!(
        resolve_path("@/path/to/img.png"),
        std::path::PathBuf::from("/path/to/img.png")
    );
    assert_eq!(
        resolve_path("\"@/path/to/img.png\""),
        std::path::PathBuf::from("/path/to/img.png")
    );
    assert_eq!(
        resolve_path("'/path/to/img.png'"),
        std::path::PathBuf::from("/path/to/img.png")
    );
}

#[test]
fn test_extract_image_references_with_real_tempfile() {
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("hq_test_screenshot.png");
    {
        let mut f = File::create(&temp_file).expect("create temp image");
        f.write_all(b"fake png content").expect("write content");
    }

    let input = format!(
        "Please check @{} and explain the layout bug",
        temp_file.display()
    );

    let attachments = extract_image_references(&input);
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].path, temp_file);
    assert!(attachments[0].exists);
    assert_eq!(attachments[0].filename, "hq_test_screenshot.png");
    assert_eq!(attachments[0].mime_type, "image/png");
    assert_eq!(attachments[0].size_bytes, 16);

    let desc = describe_attachments(&attachments);
    assert!(desc.contains("hq_test_screenshot.png"));
    assert!(desc.contains("16 B"));

    let formatted = format_prompt_with_images(&input, &attachments, &HarnessKind::Codex);
    assert!(formatted.contains(&format!("@{}", temp_file.display())));

    let _ = std::fs::remove_file(temp_file);
}

#[test]
fn test_image_only_prompt_formats_continuation() {
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("orca-paste-123.png");
    {
        let mut f = File::create(&temp_file).expect("create temp image");
        f.write_all(b"fake bytes").expect("write bytes");
    }

    let input = format!("@{}", temp_file.display());
    let attachments = extract_image_references(&input);
    assert_eq!(attachments.len(), 1);

    let formatted = format_prompt_with_images(&input, &attachments, &HarnessKind::Pi);
    assert!(formatted.contains("[Attached Image:"));
    assert!(formatted.contains("Please inspect the attached image(s) and proceed."));

    let _ = std::fs::remove_file(temp_file);
}
