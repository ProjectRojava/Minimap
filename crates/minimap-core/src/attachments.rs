//! What may be attached to an item (spec 22): the allowed kinds, a safe display name, and the
//! Markdown that shows an attachment in a note. Pure; the store keeps the rows and the sync
//! crate keeps the bytes.

use minimap_types::{AttachmentKind, Uuid, MAX_ATTACHMENT_BYTES};

/// (extension, kind, MIME type). Lowercase, without the dot. Nothing executable or scriptable
/// is on the list; SVG is allowed because it is only ever shown as an image.
const ALLOWED: &[(&str, AttachmentKind, &str)] = &[
    ("png", AttachmentKind::Image, "image/png"),
    ("jpg", AttachmentKind::Image, "image/jpeg"),
    ("jpeg", AttachmentKind::Image, "image/jpeg"),
    ("gif", AttachmentKind::Image, "image/gif"),
    ("webp", AttachmentKind::Image, "image/webp"),
    ("bmp", AttachmentKind::Image, "image/bmp"),
    ("avif", AttachmentKind::Image, "image/avif"),
    ("heic", AttachmentKind::Image, "image/heic"),
    ("tif", AttachmentKind::Image, "image/tiff"),
    ("tiff", AttachmentKind::Image, "image/tiff"),
    ("svg", AttachmentKind::Svg, "image/svg+xml"),
    ("md", AttachmentKind::Text, "text/markdown"),
    ("markdown", AttachmentKind::Text, "text/markdown"),
    ("txt", AttachmentKind::Text, "text/plain"),
    ("csv", AttachmentKind::Text, "text/csv"),
    ("tsv", AttachmentKind::Text, "text/tab-separated-values"),
    ("json", AttachmentKind::Text, "application/json"),
    ("pdf", AttachmentKind::Pdf, "application/pdf"),
    ("doc", AttachmentKind::Word, "application/msword"),
    (
        "docx",
        AttachmentKind::Word,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    ("dot", AttachmentKind::Word, "application/msword"),
    (
        "dotx",
        AttachmentKind::Word,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
    ),
    (
        "odt",
        AttachmentKind::Word,
        "application/vnd.oasis.opendocument.text",
    ),
    ("rtf", AttachmentKind::Word, "application/rtf"),
    ("xls", AttachmentKind::Excel, "application/vnd.ms-excel"),
    (
        "xlsx",
        AttachmentKind::Excel,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (
        "xlsm",
        AttachmentKind::Excel,
        "application/vnd.ms-excel.sheet.macroEnabled.12",
    ),
    (
        "ods",
        AttachmentKind::Excel,
        "application/vnd.oasis.opendocument.spreadsheet",
    ),
    (
        "ppt",
        AttachmentKind::PowerPoint,
        "application/vnd.ms-powerpoint",
    ),
    (
        "pptx",
        AttachmentKind::PowerPoint,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    (
        "odp",
        AttachmentKind::PowerPoint,
        "application/vnd.oasis.opendocument.presentation",
    ),
    (
        "key",
        AttachmentKind::PowerPoint,
        "application/vnd.apple.keynote",
    ),
];

/// The longest display name kept, in characters.
const MAX_NAME_CHARS: usize = 200;

/// The extension of `name` in lowercase, if it has one.
fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// The kind and MIME type for a file name, or a sentence saying why it isn't accepted.
pub fn classify(file_name: &str) -> Result<(AttachmentKind, &'static str), String> {
    let Some(ext) = extension(file_name) else {
        return Err(format!(
            "\"{file_name}\" has no file extension, so Minimap can't tell what it is. {}",
            allowed_summary()
        ));
    };
    ALLOWED
        .iter()
        .find(|(e, _, _)| *e == ext)
        .map(|(_, kind, mime)| (*kind, *mime))
        .ok_or_else(|| format!("Minimap can't keep .{ext} files. {}", allowed_summary()))
}

/// One sentence listing what is accepted.
pub fn allowed_summary() -> &'static str {
    "Allowed: images (including SVG), Markdown and text, PDF, Word, Excel and PowerPoint files"
}

/// Checks the size of a file about to be attached.
pub fn check_size(bytes: u64) -> Result<(), String> {
    if bytes == 0 {
        return Err("That file is empty".to_owned());
    }
    if bytes > MAX_ATTACHMENT_BYTES {
        return Err(format!(
            "That file is {} MB; the limit is {} MB",
            bytes.div_ceil(1024 * 1024),
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

/// A name that is safe to show and to use as a download name: no folders, no control characters,
/// at most 200 characters (the extension is kept when it has to be cut).
pub fn clean_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_owned();
    if cleaned.chars().count() <= MAX_NAME_CHARS {
        return cleaned;
    }
    let ext = extension(&cleaned)
        .filter(|e| e.chars().count() <= 10)
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let keep = MAX_NAME_CHARS - ext.chars().count();
    let stem: String = cleaned.chars().take(keep).collect();
    format!("{stem}{ext}")
}

/// What to put in a note to show the attachment: a picture for images, a link otherwise.
pub fn markdown(id: Uuid, file_name: &str, kind: AttachmentKind) -> String {
    let label: String = file_name
        .chars()
        .map(|c| {
            if matches!(c, '[' | ']' | '(' | ')' | '\\') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let bang = if kind.is_picture() { "!" } else { "" };
    format!("{bang}[{}](attachment:{id})", label.trim())
}

/// Is `s` a lowercase hex SHA-256?
pub fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_the_owner_asked_for_are_accepted() {
        let cases = [
            ("diagram.PNG", AttachmentKind::Image),
            ("photo.jpeg", AttachmentKind::Image),
            ("logo.svg", AttachmentKind::Svg),
            ("notes.md", AttachmentKind::Text),
            ("budget.xlsx", AttachmentKind::Excel),
            ("old.xls", AttachmentKind::Excel),
            ("deck.pptx", AttachmentKind::PowerPoint),
            ("spec.docx", AttachmentKind::Word),
            ("contract.pdf", AttachmentKind::Pdf),
        ];
        for (name, kind) in cases {
            assert_eq!(classify(name).unwrap().0, kind, "{name}");
        }
    }

    #[test]
    fn programs_scripts_and_unknown_files_are_refused_with_a_reason() {
        for name in [
            "setup.exe",
            "run.sh",
            "page.html",
            "macro.js",
            "installer.msi",
            "archive.zip",
            "noextension",
            ".hidden",
            "trailing.",
        ] {
            let why = classify(name).unwrap_err();
            assert!(why.contains("Allowed:"), "{name}: {why}");
        }
    }

    #[test]
    fn the_pickers_list_matches_the_kind_table() {
        let table: Vec<&str> = ALLOWED.iter().map(|(e, _, _)| *e).collect();
        assert_eq!(table, minimap_types::ATTACHMENT_EXTENSIONS);
        for ext in minimap_types::ATTACHMENT_EXTENSIONS {
            assert!(classify(&format!("file.{ext}")).is_ok(), "{ext}");
        }
    }

    #[test]
    fn sizes_are_checked() {
        assert!(check_size(1).is_ok());
        assert!(check_size(MAX_ATTACHMENT_BYTES).is_ok());
        assert!(check_size(0).unwrap_err().contains("empty"));
        assert!(check_size(MAX_ATTACHMENT_BYTES + 1)
            .unwrap_err()
            .contains("250 MB"));
    }

    #[test]
    fn names_lose_folders_and_control_characters() {
        assert_eq!(clean_name("/home/me/Reports/q1.xlsx"), "q1.xlsx");
        assert_eq!(clean_name("C:\\Users\\me\\q1.xlsx"), "q1.xlsx");
        assert_eq!(clean_name("  a\u{7}b.png "), "ab.png");
        assert_eq!(clean_name("../../etc/passwd.txt"), "passwd.txt");
        assert_eq!(clean_name("...dots.md"), "dots.md");
    }

    #[test]
    fn long_names_are_cut_but_keep_their_extension() {
        let long = format!("{}.docx", "x".repeat(500));
        let cleaned = clean_name(&long);
        assert_eq!(cleaned.chars().count(), 200);
        assert!(cleaned.ends_with(".docx"));
    }

    #[test]
    fn markdown_shows_pictures_and_links_the_rest() {
        let id = Uuid::from_u128(7);
        assert_eq!(
            markdown(id, "map [v2].png", AttachmentKind::Image),
            format!("![map  v2 .png](attachment:{id})")
        );
        assert_eq!(
            markdown(id, "plan.pdf", AttachmentKind::Pdf),
            format!("[plan.pdf](attachment:{id})")
        );
    }

    #[test]
    fn sha256_text_is_recognised() {
        assert!(is_sha256(&"ab12".repeat(16)));
        assert!(!is_sha256(&"AB12".repeat(16)));
        assert!(!is_sha256("short"));
        assert!(!is_sha256(&"g".repeat(64)));
    }
}
