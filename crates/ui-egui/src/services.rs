use super::state::{ImportRequest, ImportedFile};

pub type ReadFn = Box<dyn Fn(&str) -> Result<Vec<u8>, String>>;

pub type WriteFn = Box<dyn FnMut(&str, &[u8]) -> Result<(), String>>;

pub type OpenAsyncFn = Box<dyn FnMut(ImportRequest)>;

/// Starts a browser download; an error means no file was handed to the user.
pub type DownloadFn = Box<dyn FnMut(&str, &[u8]) -> Result<(), String>>;

/// A save of the document itself, handed to [`Services::write_document`].
#[derive(Debug, Clone, Copy)]
pub struct DocumentWrite<'a> {
    /// Names this save for [`crate::DesignApp::confirm_document_write`], once the shell knows the
    /// file was written.
    pub id: u64,
    /// The file name the document is saved under: its own file, or `<title>.designcraft`.
    pub path: &'a str,
    /// The `.designcraft` file.
    pub bytes: &'a [u8],
    /// `false` when the document is written over its own file (Save of a document that has one);
    /// `true` for Save As (even under the same name) and the first save of a new or converted
    /// document, where the shell asks where to put it.
    pub save_as: bool,
}

/// [`Services::write_document`].
pub type WriteDocumentFn = Box<dyn FnMut(DocumentWrite<'_>) -> Result<(), String>>;

pub type ClipboardFn = Box<dyn FnMut() -> Option<String>>;

/// Keeps the window's frame as the app needs it; called once per frame, before it is laid out.
pub type WindowFrameFn = Box<dyn FnMut()>;

/// Where the pointer is on the screen now, in the system's points from the top left of the main
/// display, before the UI scale: the app divides them by egui's zoom factor to compare them with
/// the window's `inner_rect`. `None` when the system doesn't say.
pub type PointerFn = Box<dyn Fn() -> Option<egui::Pos2>>;

/// Files delivered asynchronously with their original operation and document identity.
pub type Inbox = std::sync::Arc<std::sync::Mutex<Vec<ImportedFile>>>;

/// Platform services injected by the host (desktop or web). File dialogs aren't among them: the
/// app asks for them itself and the host shows them (`DesignApp::show_file_dialogs`).
#[derive(Default)]
pub struct Services {
    pub read: Option<ReadFn>,
    pub write: Option<WriteFn>,
    /// Asynchronous open dialog for a purpose (`open`, `place`); the chosen file arrives later
    /// through [`Services::inbox`]. In a browser, which has no native dialogs.
    pub open_async: Option<OpenAsyncFn>,
    /// Hand bytes to the user as a named file (browser download). When set, Save uses it
    /// instead of writing to a path, and a file saved under a name goes to the downloads with no
    /// save dialog.
    pub download: Option<DownloadFn>,
    /// Files delivered asynchronously, drained every frame with their captured request context.
    pub inbox: Option<Inbox>,
    /// Writes the saves of the document itself (Save, Save As), for a shell that keeps the
    /// document's file apart from copies and exports: a page that hosts the web build stores the
    /// document, and `write` and `download` still take everything else. A document handed over is
    /// not marked saved: it stays modified until the shell calls
    /// [`crate::DesignApp::confirm_document_write`] (the host says the file was stored), and an
    /// action waiting for the save (Save, then close) runs then. `None`: saves go as before.
    pub write_document: Option<WriteDocumentFn>,
    /// The app runs inside a page that hosts it (the web build's host mode): About, the Help menu,
    /// the app bar and the start screen leave out the ArtCraft community and update links.
    pub hosted: bool,
    /// The system clipboard's text, for Paste chosen from a menu (egui only delivers it with the
    /// paste keys, which a native menu bar takes first).
    pub clipboard_text: Option<ClipboardFn>,
    /// Keeps the undecorated window borderless, so the content and the pointer stay aligned
    /// (Windows custom title bar, #1295). `None` when the window is decorated or the platform
    /// needs no fixing.
    pub window_frame: Option<WindowFrameFn>,
    /// The pointer's place on the screen, asked when a file is dropped: a file dragged in from
    /// another application brings no pointer events with it (winit reports none during a drag on
    /// macOS), so without it a drop can't tell which frame it landed on. `None`: a drop without
    /// pointer events lands in the middle of the view.
    pub pointer: Option<PointerFn>,
}
