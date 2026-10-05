//! Files attached to items (spec 22): adding (from the file picker, or pasted and dropped data),
//! listing, removing, opening with another program, and the `minimap-media` protocol that shows
//! attached pictures in notes. The rows are in `minimap-store`; the bytes are encrypted in the
//! local cache and, when Drive is connected, on Drive (`minimap-sync`).
//!
//! The vault key is made the first time something is attached, and everything stored is
//! encrypted with it. Nothing here logs file names or contents.

use std::{
    fs,
    path::{Path, PathBuf},
};

use minimap_core::attachments::{check_size, classify, clean_name};
use minimap_store::{attachments, meta, security::secure_remove};
use minimap_sync::{media::MediaCache, Engine};
use minimap_types::Uuid;
use minimap_types::{AddAttachment, AppError, Attachment, AttachmentState, NodeRef, NodeType};
use tauri::{ipc::InvokeBody, State};

use crate::{
    commands::sync::sync_error,
    error::{app_error, store_error},
    state::AppState,
};

fn invalid(message: impl std::fmt::Display) -> AppError {
    app_error("invalid", message)
}

/// Where an attachment's bytes are, for the list.
fn state_of(engine: &Engine, cache: &MediaCache, a: &Attachment) -> AttachmentState {
    if !engine.is_connected() {
        return AttachmentState::LocalOnly;
    }
    let here = cache.has(&a.sha256);
    match (engine.is_on_remote(&a.sha256), here) {
        (true, true) => AttachmentState::OnDrive,
        (true, false) => AttachmentState::NotDownloaded,
        (false, true) => AttachmentState::Waiting,
        (false, false) => AttachmentState::NotDownloaded,
    }
}

fn with_state(state: &AppState, mut list: Vec<Attachment>) -> Vec<Attachment> {
    let cache = state.sync.engine.media_cache();
    for a in &mut list {
        a.state = state_of(&state.sync.engine, &cache, a);
    }
    list
}

/// An item's attachments, oldest first.
#[tauri::command]
pub async fn list_attachments(
    state: State<'_, AppState>,
    node: NodeRef,
) -> Result<Vec<Attachment>, AppError> {
    let list = state
        .run(move |conn| attachments::list_for_node(conn, node.id, false).map_err(store_error))
        .await?;
    Ok(with_state(&state, list))
}

enum Source {
    Path(PathBuf),
    Bytes(Vec<u8>),
}

/// Checks, encrypts into the cache (outside the database lock), and records the attachment.
async fn store_attachment(
    state: &AppState,
    node: NodeRef,
    raw_name: &str,
    source: Source,
) -> Result<Attachment, AppError> {
    let name = clean_name(raw_name);
    classify(&name).map_err(invalid)?;
    let size = match &source {
        Source::Path(p) => fs::metadata(p)
            .map_err(|e| invalid(format!("Couldn't read that file: {e}")))?
            .len(),
        Source::Bytes(b) => b.len() as u64,
    };
    check_size(size).map_err(invalid)?;
    let key = state
        .run(|conn| meta::ensure_vault_key(conn).map_err(store_error))
        .await?;
    let cache = state.sync.engine.media_cache();
    let sealed = tauri::async_runtime::spawn_blocking(move || match source {
        Source::Path(p) => cache.add_file(&key, &p),
        Source::Bytes(b) => cache.add_bytes(&key, &b),
    })
    .await
    .map_err(|e| app_error("internal", e))?
    .map_err(sync_error)?;
    let added = {
        let name = name.clone();
        state
            .run(move |conn| {
                attachments::add(conn, node, &name, sealed.plain_bytes, &sealed.sha256)
                    .map_err(store_error)
            })
            .await?
    };
    Ok(with_state(state, vec![added]).remove(0))
}

/// Attaches the file at `path` (picked in the file dialog) to an item.
#[tauri::command]
pub async fn add_attachment(
    state: State<'_, AppState>,
    request: AddAttachment,
) -> Result<Attachment, AppError> {
    let path = PathBuf::from(&request.path);
    if !path.is_absolute() {
        return Err(invalid("The file's path must be a full path"));
    }
    if !path.is_file() {
        return Err(invalid("That isn't a file"));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    store_attachment(&state, request.node, &name, Source::Path(path)).await
}

/// Attaches data that arrived as bytes (a picture pasted into a note, a file dropped on the
/// window). The item and the file name travel in the request headers (`x-node-type`,
/// `x-node-id`, `x-file-name`, the name percent-encoded); the body is the file.
#[tauri::command]
pub async fn add_attachment_data(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<Attachment, AppError> {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| invalid(format!("The request is missing {name}")))
    };
    let node_type: NodeType = header("x-node-type")?
        .parse()
        .map_err(|_| invalid("Unknown kind of item"))?;
    let node_id = Uuid::parse_str(&header("x-node-id")?).map_err(|_| invalid("Not an item id"))?;
    let name = percent_decode(&header("x-file-name")?);
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(invalid("The file's bytes are missing"));
    };
    store_attachment(
        &state,
        NodeRef::new(node_type, node_id),
        &name,
        Source::Bytes(bytes.clone()),
    )
    .await
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Removes an attachment from its item. (Its file goes from Drive and from the cache once
/// nothing refers to it.)
#[tauri::command]
pub async fn remove_attachment(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| attachments::remove(conn, id).map_err(store_error))
        .await
}

/// Opens an attachment with the program the system uses for that kind of file. A decrypted copy
/// is made in the app's `open` folder, which is emptied each time Minimap starts.
#[tauri::command]
pub async fn open_attachment(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    let (row, key) = state
        .run(move |conn| {
            let row = attachments::get(conn, id).map_err(store_error)?;
            let key = meta::vault_key(conn).map_err(store_error)?;
            Ok((row, key))
        })
        .await?;
    let key =
        key.ok_or_else(|| app_error("state", "There is no key for this device's attachments"))?;
    let engine = state.sync.engine.clone();
    let dir = state.data_dir.join("open").join(id.to_string());
    tauri::async_runtime::spawn_blocking(move || -> Result<(), AppError> {
        if !engine.media_cache().has(&row.sha256) {
            engine.fetch_media(&row.sha256).map_err(sync_error)?;
        }
        fs::create_dir_all(&dir)
            .map_err(|e| app_error("io", format!("Couldn't make a folder: {e}")))?;
        let target = dir.join(clean_name(&row.file_name));
        engine
            .media_cache()
            .export_plain(&key, &row.sha256, &target)
            .map_err(sync_error)?;
        open::that_detached(&target)
            .map_err(|e| app_error("io", format!("Couldn't open the file: {e}")))
    })
    .await
    .map_err(|e| app_error("internal", e))?
}

/// Deletes the decrypted copies left from the last run.
pub fn clean_opened(data_dir: &Path) {
    let root = data_dir.join("open");
    let Ok(read) = fs::read_dir(&root) else {
        return;
    };
    for dir in read.flatten() {
        if let Ok(files) = fs::read_dir(dir.path()) {
            for f in files.flatten() {
                secure_remove(&f.path());
            }
        }
        let _ = fs::remove_dir(dir.path());
    }
    let _ = fs::remove_dir(&root);
}

// ------------------------------------------------------------------ the minimap-media protocol

fn plain_response(
    status: u16,
    body: Vec<u8>,
    content_type: &str,
) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("X-Content-Type-Options", "nosniff")
        // A picture can't run anything even if it is an SVG with a script in it.
        .header(
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline'; sandbox",
        )
        .header("Cache-Control", "private, max-age=3600")
        .body(body)
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

/// Serves an attached picture to the webview for `minimap-media://localhost/<attachment id>`.
/// Only pictures, only attachments that exist and are not removed; the bytes are decrypted from
/// the cache (fetched from Drive first when this device doesn't have them).
pub fn serve_media(state: &AppState, uri_path: &str) -> tauri::http::Response<Vec<u8>> {
    let not_found = || plain_response(404, Vec::new(), "text/plain");
    let Ok(id) = Uuid::parse_str(uri_path.trim_matches('/')) else {
        return not_found();
    };
    let (row, key) = {
        let Ok(mut guard) = state.db.lock() else {
            return not_found();
        };
        let Some(conn) = guard.conn.as_mut() else {
            return plain_response(503, Vec::new(), "text/plain");
        };
        let Ok(row) = attachments::get(conn, id) else {
            return not_found();
        };
        let Ok(Some(key)) = meta::vault_key(conn) else {
            return not_found();
        };
        (row, key)
    };
    if row.archived_at.is_some() || !row.kind.is_picture() {
        return not_found();
    }
    let engine = &state.sync.engine;
    if !engine.media_cache().has(&row.sha256) && engine.fetch_media(&row.sha256).is_err() {
        return plain_response(502, Vec::new(), "text/plain");
    }
    match engine.media_cache().read_plain(&key, &row.sha256) {
        Ok(bytes) => plain_response(200, bytes, &row.mime_type),
        Err(_) => plain_response(500, Vec::new(), "text/plain"),
    }
}

#[cfg(test)]
mod tests {
    use minimap_store::{nodes, tasks};
    use minimap_types::{AssigneeChoice, CreateTask};

    use super::*;
    use crate::commands::sync::tests::app;

    fn a_task(state: &AppState, title: &str) -> NodeRef {
        let mut guard = state.db.lock().unwrap();
        let conn = guard.conn.as_mut().unwrap();
        let task = tasks::create(
            conn,
            CreateTask {
                title: title.into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: None,
                priority: None,
            },
        )
        .unwrap();
        NodeRef::new(NodeType::Task, task.id)
    }

    fn attach(
        state: &AppState,
        node: NodeRef,
        name: &str,
        bytes: &[u8],
    ) -> Result<Attachment, AppError> {
        tauri::async_runtime::block_on(store_attachment(
            state,
            node,
            name,
            Source::Bytes(bytes.to_vec()),
        ))
    }

    #[test]
    fn a_picture_is_stored_encrypted_served_to_the_webview_and_removed() {
        let (state, dir) = app("attach-picture");
        let task = a_task(&state, "Plan");
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg'><text>secret roadmap</text></svg>";
        let added = attach(&state, task, "/some/folder/roadmap.svg", svg).unwrap();
        assert_eq!(added.file_name, "roadmap.svg");
        assert_eq!(added.state, AttachmentState::LocalOnly);
        assert!(added.markdown.starts_with("![roadmap.svg](attachment:"));

        // On disk it is encrypted.
        let blob = std::fs::read(dir.join("media").join(format!("{}.bin", added.sha256))).unwrap();
        assert!(!blob.windows(6).any(|w| w == b"secret"));

        // The webview gets the picture, with headers that keep an SVG from running anything.
        let ok = serve_media(&state, &format!("/{}", added.id));
        assert_eq!(ok.status(), 200);
        assert_eq!(ok.body().as_slice(), svg.as_slice());
        assert_eq!(ok.headers()["content-type"], "image/svg+xml");
        assert_eq!(ok.headers()["x-content-type-options"], "nosniff");
        assert!(ok.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox"));

        // Unknown ids and paths that aren't ids are 404.
        assert_eq!(
            serve_media(&state, &format!("/{}", Uuid::now_v7())).status(),
            404
        );
        assert_eq!(serve_media(&state, "/../../etc/passwd").status(), 404);
        assert_eq!(serve_media(&state, "/").status(), 404);

        // It is listed, then removed, and then no longer served.
        let listed = with_state(&state, {
            let guard = state.db.lock().unwrap();
            attachments::list_for_node(guard.conn.as_ref().unwrap(), task.id, false).unwrap()
        });
        assert_eq!(listed.len(), 1);
        {
            let mut guard = state.db.lock().unwrap();
            attachments::remove(guard.conn.as_mut().unwrap(), added.id).unwrap();
        }
        assert_eq!(serve_media(&state, &format!("/{}", added.id)).status(), 404);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_pictures_are_served_inline_and_files_open_with_other_programs() {
        let (state, dir) = app("attach-docs");
        let task = a_task(&state, "Plan");
        for (name, bytes) in [
            ("budget.xlsx", &b"PK sheet"[..]),
            ("notes.md", b"# notes"),
            ("deck.pptx", b"PK deck"),
        ] {
            let a = attach(&state, task, name, bytes).unwrap();
            assert_eq!(
                serve_media(&state, &format!("/{}", a.id)).status(),
                404,
                "{name} is not a picture"
            );
            assert!(a.markdown.starts_with(&format!("[{name}](attachment:")));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn files_that_are_not_allowed_are_refused_and_nothing_is_kept() {
        let (state, dir) = app("attach-refused");
        let task = a_task(&state, "Plan");
        for (name, bytes) in [
            ("setup.exe", &b"MZ"[..]),
            ("run.sh", b"#!/bin/sh"),
            ("empty.png", b""),
            ("noext", b"x"),
        ] {
            let e = attach(&state, task, name, bytes).unwrap_err();
            assert_eq!(e.code, "invalid", "{name}: {e:?}");
        }
        // Too big: a sparse file just over the limit, so no memory is used.
        let big = dir.join("huge.png");
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(minimap_types::MAX_ATTACHMENT_BYTES + 1).unwrap();
        let e = tauri::async_runtime::block_on(store_attachment(
            &state,
            task,
            "huge.png",
            Source::Path(big),
        ))
        .unwrap_err();
        assert!(e.message.contains("250 MB"), "{}", e.message);
        // Nothing attached, nothing cached, and a missing file is a clear error.
        let none = {
            let guard = state.db.lock().unwrap();
            attachments::list_for_node(guard.conn.as_ref().unwrap(), task.id, true).unwrap()
        };
        assert!(none.is_empty());
        assert_eq!(state.sync.engine.media_cache().shas().len(), 0);
        let e = tauri::async_runtime::block_on(store_attachment(
            &state,
            task,
            "gone.png",
            Source::Path(dir.join("gone.png")),
        ))
        .unwrap_err();
        assert_eq!(e.code, "invalid");
        // Archived items take no attachments.
        {
            let mut guard = state.db.lock().unwrap();
            nodes::archive(guard.conn.as_mut().unwrap(), task).unwrap();
        }
        assert!(attach(&state, task, "ok.png", b"png").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_same_file_is_one_blob_and_names_survive_percent_encoding() {
        let (state, dir) = app("attach-dedupe");
        let (a, b) = (a_task(&state, "A"), a_task(&state, "B"));
        let first = attach(&state, a, "logo.png", b"pixels").unwrap();
        let second = attach(&state, b, "logo copy.png", b"pixels").unwrap();
        assert_eq!(first.sha256, second.sha256);
        assert_ne!(first.id, second.id, "two attachments, one stored file");
        assert_eq!(state.sync.engine.media_cache().shas().len(), 1);
        // Headers carry names percent-encoded.
        assert_eq!(
            percent_decode("Q3%20plan%20%28final%29.pdf"),
            "Q3 plan (final).pdf"
        );
        assert_eq!(percent_decode("caf%C3%A9.md"), "café.md");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn decrypted_copies_for_other_programs_are_cleaned_up_at_start() {
        let (_state, dir) = app("attach-clean");
        let open_dir = dir.join("open").join("some-id");
        std::fs::create_dir_all(&open_dir).unwrap();
        std::fs::write(open_dir.join("budget.xlsx"), b"decrypted").unwrap();
        clean_opened(&dir);
        assert!(!dir.join("open").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
