//! Host mode: a page that runs DesignCraft (as an iframe, or in its own document) opens files in it
//! and keeps the document's saves, instead of the browser picker and downloads. The messages and
//! the two transports are `craft-web-host`'s (storytold/craft-libs), the contract the other Craft
//! web builds share, with the `designcraft:` prefix.
//!
//! - `?host=parent` (an `<iframe>`) talks to the same-origin parent over `postMessage`;
//!   `data-host="page"` on the canvas (the page's own document) uses DOM `CustomEvent`s on the
//!   canvas. With no parent, or a parent on another origin, there is no bridge: the normal app.
//! - `designcraft:ready { version, appVersion, file: false }` once the app listens: send a file
//!   now. `designcraft:failed { error }` when the app couldn't start, couldn't open a file, or
//!   lost its graphics (the "Graphics connection lost" panel).
//! - `designcraft:open { name, bytes }` opens the file in a new tab, as File › Open does. A
//!   `.designcraft` keeps its name as its file, so File › Save writes back to it; an IDML or
//!   InDesign file is converted and has none. A file over [`MAX_OPEN_BYTES`] is refused before it
//!   is copied.
//! - `designcraft:save { name, bytes, saveAs, id }` for each save of the document itself (Save,
//!   Save As, Save before closing). `saveAs` is `false` when the document is written over its own
//!   file, `true` for Save As and the first save of a new or converted document. The document is
//!   saved only when the host answers `designcraft:saved { id }`; until then it stays modified,
//!   and what waited for the save (closing the document) waits too. Copies and exports still
//!   download.
//! - `designcraft:dirty { dirty }` when unsaved changes appear in any open document or go away.

use std::cell::RefCell;
use std::rc::Rc;

use craft_web_host::web::{FileData, Host, Message};
use designcraft_ui_egui::{DesignApp, DocumentWrite, Services};

use crate::hostmode::{MAX_OPEN_BYTES, accepts_size, file_name, parse_save_id, save_id};

/// The app id in the host messages (`designcraft:ready`).
const APP: &str = "designcraft";

thread_local! {
    /// The host, for the graphics failure path (`web::graphics_failed`), which may run while the
    /// app is borrowed and so can't reach the bridge through it.
    static FAILED_HOST: RefCell<Option<Rc<Host>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Queues {
    /// Files from the host, for the next frame: (name, bytes).
    opens: Vec<(String, Vec<u8>)>,
    /// `saved` ids from the host, for the next frame.
    saved: Vec<String>,
}

#[derive(Clone)]
pub struct Bridge {
    host: Rc<Host>,
    queues: Rc<RefCell<Queues>>,
}

impl Bridge {
    /// The bridge for this page, or `None` when DesignCraft isn't hosted (see the module docs).
    pub fn detect(canvas: &web_sys::HtmlCanvasElement) -> Option<Self> {
        let host = Rc::new(Host::connect(APP, canvas)?);
        FAILED_HOST.with(|h| {
            if let Ok(mut h) = h.try_borrow_mut() {
                *h = Some(host.clone());
            }
        });
        Some(Self { host, queues: Rc::default() })
    }

    /// Document saves go to the host; listen for its files and confirmations.
    pub fn connect(&self, services: &mut Services, ctx: egui::Context) {
        self.listen(ctx);
        services.hosted = true;
        let host = self.host.clone();
        services.write_document = Some(Box::new(move |write: DocumentWrite<'_>| {
            let name = file_name(write.path);
            host.post_save(&name, write.bytes, Some(&save_id(write.id)), write.save_as).map_err(|e| format!("couldn't hand {name} to the page: {e}"))
        }));
    }

    pub fn post_ready(&self) {
        if let Err(e) = self.host.post_ready(env!("CARGO_PKG_VERSION"), false) {
            log::error!("couldn't tell the page DesignCraft is ready: {e}");
        }
    }

    /// Each frame, before the app's: open the host's files, mark confirmed saves, and tell the
    /// host when unsaved changes appear or go away (the crate doesn't repeat a state).
    pub fn sync(&self, app: &mut DesignApp) {
        let (opens, saved) = {
            let Ok(mut q) = self.queues.try_borrow_mut() else { return };
            (std::mem::take(&mut q.opens), std::mem::take(&mut q.saved))
        };
        for (name, bytes) in opens {
            if let Err(e) = app.open_hosted(&name, &bytes) {
                log::error!("couldn't open {name} from the page: {e}");
                let _ = self.host.post_failed(&format!("couldn't open {name}: {e}"), None);
            }
        }
        for id in saved {
            match parse_save_id(&id) {
                Some(save) => {
                    app.confirm_document_write(save);
                }
                None => log::warn!("the page confirmed a save DesignCraft never sent: {id}"),
            }
        }
        self.host.post_dirty(app.session.documents().iter().any(|d| d.is_dirty()));
    }

    fn listen(&self, ctx: egui::Context) {
        let queues = self.queues.clone();
        let host = self.host.clone();
        self.host.listen(move |message| {
            let Ok(mut q) = queues.try_borrow_mut() else {
                log::warn!("a message from the page arrived while the last one was being read; dropped");
                return;
            };
            match message {
                Message::Open { name, data } => match take_open(&name, &data) {
                    Ok(bytes) => q.opens.push((name, bytes)),
                    Err(e) => {
                        log::warn!("{e}");
                        let _ = host.post_failed(&e, None);
                        return;
                    }
                },
                Message::Saved { id } => q.saved.push(id),
                Message::Host { .. } | Message::HandoffDone { .. } => return,
            }
            ctx.request_repaint();
        });
    }
}

/// The app can't go on (graphics lost): tell the host. Called from the failure path; touches
/// nothing of the app's.
pub fn post_failed(detail: &str) {
    FAILED_HOST.with(|h| {
        if let Ok(h) = h.try_borrow()
            && let Some(host) = h.as_ref()
        {
            let _ = host.post_failed(detail, None);
        }
    });
}

/// The bytes an open message carries. A host's file is untrusted input: an oversized one is
/// refused before it is copied into the module's memory (the data's own length, not the host's
/// claim).
fn take_open(name: &str, data: &FileData) -> Result<Vec<u8>, String> {
    let FileData::Bytes(array) = data else {
        return Err(format!("{name} came as a File; DesignCraft reads bytes only (ready says file: false)"));
    };
    if !accepts_size(data.size()) {
        return Err(format!("{name} is {} bytes, over the {} MiB limit for a file from the page", data.size(), MAX_OPEN_BYTES >> 20));
    }
    Ok(array.to_vec())
}
