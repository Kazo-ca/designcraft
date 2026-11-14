//! Pure host-mode rules, shared by the bridge (`host`) and testable without a browser: the size
//! limit for a host's file and the ids a save is handed over and confirmed with. Which pages host
//! the app is `craft-web-host`'s; what a confirmed save marks saved is the app's
//! (`DesignApp::confirm_document_write`).

/// The largest file taken from the host page, refused before it is copied into the module: wasm32
/// has 4 GiB in all, and opening a document needs several times the file.
pub const MAX_OPEN_BYTES: u64 = 1 << 30;

/// Whether a file of `size` bytes from the host may be opened.
pub fn accepts_size(size: f64) -> bool {
    size.is_finite() && size >= 0.0 && size <= MAX_OPEN_BYTES as f64
}

/// The id a save is handed to the host with (`designcraft:save { id }`).
pub fn save_id(save: u64) -> String {
    format!("save-{save}")
}

/// The app's save from the id the host answers with (`designcraft:saved { id }`); `None` for an
/// id the app never gave.
pub fn parse_save_id(id: &str) -> Option<u64> {
    id.strip_prefix("save-")?.parse().ok()
}

/// The file name of a save path (the web's paths are names already; a folder is dropped).
pub fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_file_over_the_limit_or_with_a_bad_size_is_refused() {
        assert!(accepts_size(0.0));
        assert!(accepts_size(MAX_OPEN_BYTES as f64));
        assert!(!accepts_size(MAX_OPEN_BYTES as f64 + 1.0));
        assert!(!accepts_size(f64::NAN));
        assert!(!accepts_size(-1.0));
        assert!(!accepts_size(f64::INFINITY));
    }

    #[test]
    fn save_ids_go_to_the_host_and_come_back() {
        assert_eq!(save_id(7), "save-7");
        assert_eq!(parse_save_id(&save_id(7)), Some(7));
        for id in ["", "7", "save-", "save-x", "save--1", "other-7", "save-99999999999999999999999"] {
            assert_eq!(parse_save_id(id), None, "{id}");
        }
    }

    #[test]
    fn save_paths_become_file_names() {
        assert_eq!(file_name("Brochure.designcraft"), "Brochure.designcraft");
        assert_eq!(file_name("dir/Brochure.designcraft"), "Brochure.designcraft");
    }
}
