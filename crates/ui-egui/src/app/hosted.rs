//! A shell that keeps the document's file itself ([`Services::write_document`], the web build's
//! host mode): files it hands over open under their name, saves of the document go to it, and a
//! document counts as saved only once the shell confirms the write.

use std::collections::VecDeque;
use std::sync::Arc;

use designcraft_doc::Document;
use serde_json::{Value, json};

use super::DesignApp;
use crate::services::DocumentWrite;

/// What runs once a save is confirmed (Save, then close the document).
pub(crate) type AfterSave = Box<dyn FnOnce(&mut DesignApp) -> Result<Value, String>>;

/// Saves handed to the shell and not confirmed yet, at most: a shell that never answers can't make
/// the list grow without end (the oldest is forgotten, and its document stays modified).
pub(crate) const MAX_WAITING_SAVES: usize = 64;

/// A save handed to the shell: what its confirmation marks saved.
struct Waiting {
    id: u64,
    uid: u64,
    epoch: u64,
    doc: Arc<Document>,
    revision: u64,
    path: String,
    after: Option<AfterSave>,
}

/// The saves waiting for the shell's confirmation.
#[derive(Default)]
pub(crate) struct HostedSaves {
    next: u64,
    waiting: VecDeque<Waiting>,
}

impl HostedSaves {
    fn add(&mut self, mut save: Waiting) -> u64 {
        self.next = self.next.saturating_add(1);
        save.id = self.next;
        if self.waiting.len() >= MAX_WAITING_SAVES {
            self.waiting.pop_front();
        }
        self.waiting.push_back(save);
        self.next
    }

    fn take(&mut self, id: u64) -> Option<Waiting> {
        let at = self.waiting.iter().position(|w| w.id == id)?;
        self.waiting.remove(at)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.waiting.len()
    }
}

/// `name` as a document file name: with the `.designcraft` extension.
fn document_name(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".designcraft") { name.to_string() } else { format!("{name}.designcraft") }
}

impl DesignApp {
    /// Opens a file the shell hands over (a page that hosts the web build), as File › Open does, in
    /// a new tab. A DesignCraft document keeps `name` as its file, so Save writes back to it; an
    /// IDML or InDesign file is converted and keeps none (its first save asks where).
    pub fn open_hosted(&mut self, name: &str, bytes: &[u8]) -> Result<Value, String> {
        if !(super::files::opens_as_document(name) || designcraft_engine::cmd::is_document(bytes)) {
            return Err(format!("{name} is not a DesignCraft, IDML or InDesign document"));
        }
        let mut params = json!({"name": name, "base64": designcraft_engine::cmd::base64_encode(bytes)});
        if name.to_ascii_lowercase().ends_with(".designcraft") {
            params["path"] = json!(name);
        }
        self.open_file("file.openBytes", params)
    }

    /// Hands the active document to [`crate::Services::write_document`]: under its own file, or
    /// under `name` (Save As with a name), or `<title>.designcraft`. `save_as` asks the shell where
    /// to put it even when the name is its own. The document is saved, and `after` runs, only when
    /// the shell confirms the write ([`Self::confirm_document_write`]).
    pub(crate) fn hand_document(&mut self, save_as: bool, name: Option<&str>, after: Option<AfterSave>) -> Result<Value, String> {
        let st = self.session.active().ok_or("no document")?;
        let path = match name {
            Some(name) => document_name(name),
            None => st.path.clone().unwrap_or_else(|| format!("{}.designcraft", st.doc.title)),
        };
        let save_as = save_as || st.path.as_deref() != Some(path.as_str());
        let waiting = Waiting { id: 0, uid: st.uid, epoch: st.identity_epoch, doc: st.doc.clone(), revision: st.revision, path: path.clone(), after };
        let serialized = self.run("file.serialize", json!({}))?;
        let bytes = designcraft_engine::cmd::base64_decode(serialized["base64"].as_str().unwrap_or_default());
        let id = self.hosted_saves.add(waiting);
        let Some(write) = self.services.write_document.as_mut() else {
            self.hosted_saves.take(id);
            return Err("no document writer".into());
        };
        if let Err(e) = write(DocumentWrite { id, path: &path, bytes: &bytes, save_as }) {
            self.hosted_saves.take(id);
            return Err(format!("could not save {path}: {e}"));
        }
        Ok(json!({"path": path, "bytes": bytes.len(), "saveAs": save_as, "waiting": id}))
    }

    /// The save `id` handed to [`crate::Services::write_document`] reached its file: its document
    /// counts as saved under the path it was written to, at the state it was written in (later
    /// edits keep it modified), and what waited for the save runs. Returns whether a document was
    /// marked saved: not for an unknown or already confirmed id, a closed or reverted document, or
    /// one a newer confirmed save has moved on.
    pub fn confirm_document_write(&mut self, id: u64) -> bool {
        let Some(save) = self.hosted_saves.take(id) else { return false };
        let marked = self.session.mark_saved(save.uid, save.epoch, save.doc, save.revision, &save.path);
        if marked
            && let Some(after) = save.after
            && let Err(e) = after(self)
        {
            self.status(e);
        }
        marked
    }
}
