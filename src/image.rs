use crate::models::HarnessKind;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAttachment {
    pub path: PathBuf,
    pub raw_mention: String,
    pub filename: String,
    pub size_bytes: u64,
    pub mime_type: String,
    pub exists: bool,
}

impl ImageAttachment {
    pub fn formatted_size(&self) -> String {
        if self.size_bytes < 1024 {
            format!("{} B", self.size_bytes)
        } else if self.size_bytes < 1024 * 1024 {
            format!("{:.1} KB", self.size_bytes as f64 / 1024.0)
        } else {
            format!("{:.2} MB", self.size_bytes as f64 / (1024.0 * 1024.0))
        }
    }
}

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "tiff", "ico",
];

fn infer_mime_type(ext: &str) -> String {
    match ext.to_lowercase().as_str() {
        "png" => "image/png".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "bmp" => "image/bmp".to_string(),
        "tiff" => "image/tiff".to_string(),
        "ico" => "image/x-icon".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

pub fn resolve_path(raw: &str) -> PathBuf {
    let unquoted = raw.trim_matches(|c| c == '\'' || c == '"');
    let path_str = if let Some(stripped) = unquoted.strip_prefix('@') {
        stripped
    } else {
        unquoted
    };

    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }

    PathBuf::from(path_str)
}

pub fn is_image_path(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext_lower = ext.to_lowercase();
        SUPPORTED_EXTENSIONS.contains(&ext_lower.as_str())
    } else {
        false
    }
}

pub fn extract_image_references(input: &str) -> Vec<ImageAttachment> {
    let mut attachments = Vec::new();

    // Tokenize by whitespace, preserving potential quotes or @-prefixed paths
    for token in input.split_whitespace() {
        let clean = token
            .trim_matches(|c| c == ',' || c == ';' || c == '(' || c == ')' || c == '[' || c == ']');
        if clean.is_empty() {
            continue;
        }

        let resolved = resolve_path(clean);
        if is_image_path(&resolved) {
            let metadata = std::fs::metadata(&resolved).ok();
            let exists = metadata.as_ref().map(|m| m.is_file()).unwrap_or(false);
            let size_bytes = metadata.map(|m| m.len()).unwrap_or(0);
            let ext = resolved
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_string();
            let filename = resolved
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(clean)
                .to_string();

            // Avoid duplicate additions
            if !attachments
                .iter()
                .any(|a: &ImageAttachment| a.path == resolved)
            {
                attachments.push(ImageAttachment {
                    path: resolved,
                    raw_mention: clean.to_string(),
                    filename,
                    size_bytes,
                    mime_type: infer_mime_type(&ext),
                    exists,
                });
            }
        }
    }

    attachments
}

pub fn describe_attachments(images: &[ImageAttachment]) -> String {
    if images.is_empty() {
        return String::new();
    }

    if images.len() == 1 {
        let img = &images[0];
        if img.exists {
            format!("[🖼️ Image: {} ({})]", img.filename, img.formatted_size())
        } else {
            format!("[🖼️ Image: {} (not found)]", img.filename)
        }
    } else {
        let total_size: u64 = images.iter().map(|i| i.size_bytes).sum();
        let formatted_total = if total_size < 1024 * 1024 {
            format!("{:.1} KB", total_size as f64 / 1024.0)
        } else {
            format!("{:.2} MB", total_size as f64 / (1024.0 * 1024.0))
        };
        let names: Vec<String> = images.iter().map(|i| i.filename.clone()).collect();
        format!(
            "[🖼️ {} Images: {} ({})]",
            images.len(),
            names.join(", "),
            formatted_total
        )
    }
}

pub fn format_prompt_with_images(
    prompt: &str,
    images: &[ImageAttachment],
    _harness: &HarnessKind,
) -> String {
    if images.is_empty() {
        return prompt.trim().to_string();
    }

    let trimmed = prompt.trim();

    // Check if the prompt consists ONLY of image paths/mentions
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let only_images = words.iter().all(|w| {
        let p = resolve_path(w.trim_matches(|c| c == ',' || c == ';' || c == '(' || c == ')'));
        is_image_path(&p)
    });

    if trimmed.is_empty() || only_images {
        let mut parts = Vec::new();
        for img in images {
            parts.push(format!("[Attached Image: {}]", img.path.display()));
        }
        parts.push("Please inspect the attached image(s) and proceed.".to_string());
        parts.join("\n")
    } else {
        // Prompt has text. Ensure image references are explicitly tagged for clarity
        let mut result = trimmed.to_string();
        let mut missing_mentions = Vec::new();
        for img in images {
            let path_str = img.path.to_string_lossy();
            if !trimmed.contains(&img.raw_mention) && !trimmed.contains(path_str.as_ref()) {
                missing_mentions.push(format!("[Image: {}]", img.path.display()));
            }
        }
        if !missing_mentions.is_empty() {
            result.push_str("\n\n");
            result.push_str(&missing_mentions.join("\n"));
        }
        result
    }
}
