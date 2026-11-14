//! Pure browser startup, file picker and download metadata policies.

/// Query flags are case-sensitive decoded keys. Values are ignored: `?sample=false`
/// still enables sample startup, and repeated keys have the same presence semantics.
pub(crate) fn has_query_flag(query: &str, flag: &str) -> bool {
    let query = query.strip_prefix('?').unwrap_or(query);
    form_urlencoded::parse(query.as_bytes()).any(|(key, _)| key == flag)
}

/// The documents File › Open opens: DesignCraft's own, IDML and InDesign files (converted).
const DOC_EXTS: &[&str] = &["designcraft", "idml", "indd", "indt"];
/// The documents Load Styles reads styles from (`style.load` reads these two only).
const STYLE_SOURCE_EXTS: &[&str] = &["designcraft", "idml"];
/// The files File › Place puts on a spread.
const IMAGE_EXTS: &[&str] = &[
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "tif",
    "tiff",
    "bmp",
    "psd",
    "svg",
    "pdf",
    "ai",
    "txt",
    "docx",
    "rtf",
    "md",
    "xlsx",
    "idml",
    "designcraft",
];

/// The browser file picker's filter for an import `purpose` (`ImportRequest::purpose`): its label
/// and extensions. Each purpose that reads a different kind of file has its own list, so a page
/// hosting the app (which sees only the picker's `accept`) can tell File › Open from Load Styles.
pub(crate) fn picker_filter(purpose: &str) -> (&'static str, &'static [&'static str]) {
    match purpose {
        "swatches" => ("Swatch Exchange (ASE)", &["ase"]),
        "shortcuts" => ("Shortcut set (JSON, InDesign text)", &["json", "txt"]),
        "place" => ("Graphics", IMAGE_EXTS),
        styles if styles.starts_with("styles") => ("DesignCraft or IDML document", STYLE_SOURCE_EXTS),
        _ => ("DesignCraft, IDML or InDesign", DOC_EXTS),
    }
}

pub(crate) fn mime_for(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("pdf") => "application/pdf",
        Some("designcraft") => designcraft_format::MIME,
        Some("idml") => "application/vnd.adobe.indesign-idml-package",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{has_query_flag, mime_for, picker_filter};

    #[test]
    fn open_and_load_styles_pickers_accept_different_files() {
        let (_, open) = picker_filter("open");
        assert_eq!(open, &["designcraft", "idml", "indd", "indt"]);
        for styles in ["styles", "styles:paragraph", "styles:character"] {
            let (_, accepted) = picker_filter(styles);
            assert_eq!(accepted, &["designcraft", "idml"], "{styles}: style.load reads these only");
            assert_ne!(accepted, open, "{styles}: a hosting page tells the two apart by `accept`");
        }
        assert_eq!(picker_filter("swatches").1, &["ase"]);
        assert!(picker_filter("place").1.contains(&"png"));
    }

    #[test]
    fn startup_flags_match_complete_decoded_keys() {
        for flag in ["sample", "webgl"] {
            for query in [format!("?{flag}"), format!("?{flag}=false"), format!("?{flag}=1&{flag}=0"), format!("?other=value&{flag}")] {
                assert!(has_query_flag(&query, flag), "{query}");
            }
            for query in [
                String::new(),
                format!("?note={flag}"),
                format!("?{flag}d=1"),
                format!("?note=not_a_{flag}"),
                "?note=%73ample%77ebgl".to_string(),
                format!("?{flag}+extra=1"),
                format!("?{flag}%26extra=1"),
                format!("?{flag}%3Dextra=1"),
            ] {
                assert!(!has_query_flag(&query, flag), "{query}");
            }
        }
        assert!(has_query_flag("?webgl&sample", "sample"));
        assert!(has_query_flag("?webgl&sample", "webgl"));
        assert!(has_query_flag("?%73ample", "sample"));
        assert!(has_query_flag("?%77ebgl", "webgl"));
        assert!(!has_query_flag("?Sample&WEBGL", "sample"));
        assert!(!has_query_flag("?Sample&WEBGL", "webgl"));
    }

    #[test]
    fn download_mime_matches_native_package_and_existing_exports() {
        for (name, expected) in [
            ("Quarterly.designcraft", designcraft_format::MIME),
            ("日本語.multi.DEsignCraft", designcraft_format::MIME),
            ("page.pdf", "application/pdf"),
            ("page.png", "image/png"),
            ("page.jpg", "image/jpeg"),
            ("page.JPEG", "image/jpeg"),
            ("document.idml", "application/vnd.adobe.indesign-idml-package"),
            ("unknown.bin", "application/octet-stream"),
        ] {
            assert_eq!(mime_for(name), expected, "{name}");
        }
    }
}
