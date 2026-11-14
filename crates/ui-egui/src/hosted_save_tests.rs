//! A shell that keeps the document's file (`Services::write_document`, the web build's host mode):
//! a `.designcraft` handed over keeps its name, only saves of the document go to the shell, each
//! says whether it is a Save As, and the document is saved only once the shell confirms the write.

use std::{cell::RefCell, rc::Rc};

use designcraft_engine::Session;
use serde_json::{Value, json};

use crate::menus::{run_ui, save_then, shown};
use crate::{DesignApp, Services};

/// (id, path, save_as) of each document save handed over.
type Handed = Rc<RefCell<Vec<(u64, String, bool)>>>;
/// Names written or downloaded (copies, exports).
type Downloaded = Rc<RefCell<Vec<String>>>;

fn hosted_app() -> (DesignApp, Handed, Downloaded) {
    let handed: Handed = Rc::default();
    let downloaded: Downloaded = Rc::default();
    let (h, w, d) = (handed.clone(), downloaded.clone(), downloaded.clone());
    let app = DesignApp::new(
        Session::new(),
        Services {
            write: Some(Box::new(move |path, _| {
                w.borrow_mut().push(path.to_string());
                Ok(())
            })),
            download: Some(Box::new(move |path, _| {
                d.borrow_mut().push(path.to_string());
                Ok(())
            })),
            write_document: Some(Box::new(move |write| {
                assert!(designcraft_engine::cmd::is_document(write.bytes), "a .designcraft file");
                h.borrow_mut().push((write.id, write.path.to_string(), write.save_as));
                Ok(())
            })),
            hosted: true,
            ..Default::default()
        },
    );
    (app, handed, downloaded)
}

/// A one-page document's `.designcraft` bytes (`idml`: its IDML package instead).
fn file(idml: bool) -> Vec<u8> {
    let mut s = Session::new();
    s.execute("file.new", &json!({})).unwrap();
    let r = s.execute(if idml { "file.exportIdml" } else { "file.serialize" }, &json!({})).unwrap();
    designcraft_engine::cmd::base64_decode(r["base64"].as_str().unwrap())
}

fn edit(app: &mut DesignApp) {
    app.run("frame.create", json!({"rect": [36, 36, 200, 120]})).unwrap();
}

fn save(app: &mut DesignApp, id: &str) -> Value {
    run_ui(app, id, &json!({})).unwrap().unwrap()
}

fn last(handed: &Handed) -> (u64, String, bool) {
    handed.borrow().last().cloned().unwrap()
}

fn dirty(app: &DesignApp) -> bool {
    app.session.active().unwrap().is_dirty()
}

#[test]
fn a_hosted_designcraft_file_is_saved_over_itself_once_the_host_confirms() {
    let (mut app, handed, downloaded) = hosted_app();
    app.open_hosted("Brochure.designcraft", &file(false)).unwrap();
    let st = app.session.active().unwrap();
    assert_eq!((st.path.as_deref(), st.is_dirty()), (Some("Brochure.designcraft"), false), "the host's name is its file");
    edit(&mut app);
    save(&mut app, "app.save");
    let (id, path, save_as) = last(&handed);
    assert_eq!((path.as_str(), save_as), ("Brochure.designcraft", false), "written over its own file");
    assert!(downloaded.borrow().is_empty(), "the document is not downloaded");
    assert!(dirty(&app), "not saved before the host confirms");
    assert!(app.confirm_document_write(id));
    assert!(!dirty(&app));
    assert!(!app.confirm_document_write(id), "confirmed once");
    assert!(!app.confirm_document_write(id + 100), "never sent");
}

#[test]
fn an_unconfirmed_save_leaves_the_document_as_it_was() {
    let (mut app, handed, _) = hosted_app();
    app.run("file.new", json!({})).unwrap();
    edit(&mut app);
    save(&mut app, "app.save");
    assert_eq!(handed.borrow().len(), 1);
    let st = app.session.active().unwrap();
    assert_eq!((st.path.as_deref(), st.is_dirty()), (None, true), "a host that never answers (cancelled) changes nothing");
}

#[test]
fn a_converted_idml_and_a_new_document_are_first_saved_as_and_then_over_themselves() {
    for idml in [true, false] {
        let (mut app, handed, _) = hosted_app();
        if idml {
            app.open_hosted("Brochure.idml", &file(true)).unwrap();
            assert_eq!(app.session.active().unwrap().path, None, "converted: no file of its own");
        } else {
            app.run("file.new", json!({})).unwrap();
        }
        let title = app.session.active().unwrap().doc.title.clone();
        edit(&mut app);
        save(&mut app, "app.save");
        let (id, path, save_as) = last(&handed);
        assert_eq!((path, save_as), (format!("{title}.designcraft"), true), "idml: {idml}");
        // The host chose where; its confirmation names the document's file from now on.
        assert!(app.confirm_document_write(id));
        assert_eq!(app.session.active().unwrap().path.as_deref(), Some(format!("{title}.designcraft").as_str()));
        edit(&mut app);
        save(&mut app, "app.save");
        assert!(!last(&handed).2, "then written over its own file");
    }
}

#[test]
fn save_as_asks_the_host_even_under_the_same_name() {
    let (mut app, handed, _) = hosted_app();
    app.open_hosted("Brochure.designcraft", &file(false)).unwrap();
    save(&mut app, "app.saveDialog");
    assert_eq!(last(&handed).1.as_str(), "Brochure.designcraft");
    assert!(last(&handed).2, "Save As");
    run_ui(&mut app, "app.saveDialog", &json!({"path": "Other"})).unwrap().unwrap();
    assert_eq!((last(&handed).1.as_str(), last(&handed).2), ("Other.designcraft", true));
    save(&mut app, "app.save");
    assert!(!last(&handed).2, "a plain Save writes over its own file again");
}

#[test]
fn edits_after_the_hand_over_keep_the_document_modified() {
    let (mut app, handed, _) = hosted_app();
    app.open_hosted("Brochure.designcraft", &file(false)).unwrap();
    edit(&mut app);
    save(&mut app, "app.save");
    edit(&mut app);
    assert!(app.confirm_document_write(last(&handed).0));
    assert!(dirty(&app), "the later edit is not in the file");
}

#[test]
fn an_older_save_confirmed_last_does_not_move_the_document_back() {
    let (mut app, handed, _) = hosted_app();
    app.open_hosted("A.designcraft", &file(false)).unwrap();
    edit(&mut app);
    save(&mut app, "app.save");
    let first = last(&handed).0;
    edit(&mut app);
    run_ui(&mut app, "app.saveDialog", &json!({"path": "B.designcraft"})).unwrap().unwrap();
    assert!(app.confirm_document_write(last(&handed).0));
    assert!(!app.confirm_document_write(first), "a newer save was confirmed already");
    let st = app.session.active().unwrap();
    assert_eq!((st.path.as_deref(), st.is_dirty()), (Some("B.designcraft"), false));
}

#[test]
fn save_then_close_waits_for_the_host() {
    let (mut app, handed, _) = hosted_app();
    app.open_hosted("Brochure.designcraft", &file(false)).unwrap();
    edit(&mut app);
    save_then(&mut app, |app| app.run("file.close", json!({}))).unwrap();
    assert_eq!(app.session.documents().len(), 1, "still open while the host writes");
    assert!(app.confirm_document_write(last(&handed).0));
    assert!(app.session.documents().is_empty(), "closed once the file is written");
}

#[test]
fn a_save_is_confirmed_while_another_tab_is_active_and_not_after_its_tab_closed() {
    let (mut app, handed, _) = hosted_app();
    app.open_hosted("Brochure.designcraft", &file(false)).unwrap();
    edit(&mut app);
    save(&mut app, "app.save");
    let id = last(&handed).0;
    app.run("file.new", json!({})).unwrap();
    assert!(app.confirm_document_write(id), "another tab being active doesn't matter");
    assert!(!app.session.documents()[0].is_dirty());
    assert_eq!(app.session.active_index(), Some(1), "and stays active");
    app.run("file.activate", json!({"index": 0})).unwrap();
    edit(&mut app);
    save(&mut app, "app.save");
    app.run("file.close", json!({"index": 0})).unwrap();
    assert!(!app.confirm_document_write(last(&handed).0), "closed");
}

#[test]
fn exports_still_download() {
    let (mut app, handed, downloaded) = hosted_app();
    app.open_hosted("Brochure.designcraft", &file(false)).unwrap();
    run_ui(&mut app, "app.exportPdf", &json!({"bleed": true})).unwrap().unwrap();
    assert!(handed.borrow().is_empty(), "an export is not the document: {:?}", handed.borrow());
    assert_eq!(downloaded.borrow().iter().filter(|n| n.ends_with(".pdf")).count(), 1, "{:?}", downloaded.borrow());
}

#[test]
fn a_file_that_is_not_a_document_is_refused() {
    let (mut app, _, _) = hosted_app();
    assert!(app.open_hosted("photo.png", b"\x89PNG\r\n\x1a\n").is_err());
    assert!(app.session.documents().is_empty());
}

#[test]
fn a_host_that_never_answers_cannot_grow_the_waiting_list() {
    let (mut app, handed, _) = hosted_app();
    app.run("file.new", json!({})).unwrap();
    for _ in 0..crate::app::hosted::MAX_WAITING_SAVES + 3 {
        save(&mut app, "app.save");
    }
    assert_eq!(app.hosted_saves.len(), crate::app::hosted::MAX_WAITING_SAVES);
    assert!(!app.confirm_document_write(handed.borrow()[0].0), "the oldest is forgotten");
}

#[test]
fn a_hosted_app_hides_the_artcraft_links_and_the_update_check() {
    let (app, _, _) = hosted_app();
    let plain = DesignApp::new(Session::new(), Services::default());
    for id in crate::menus::HOSTED_HIDDEN {
        assert!(!shown(&app, id), "{id}");
        assert!(shown(&plain, id), "{id} outside host mode");
    }
    assert!(shown(&app, "help.about"));
}
