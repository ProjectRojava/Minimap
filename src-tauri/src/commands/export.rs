//! Full data export (spec 26): everything in open formats into a new folder, so nobody is locked
//! in. The data is read by `minimap_store::export::collect`, the Markdown made by
//! `minimap_core::export_md`; this module checks the destination and writes the files.
//!
//! The export goes into a **new** folder named by time inside the folder the user picked, built
//! next to it as `<name>.part` and renamed when complete: nothing existing is ever overwritten
//! and a failure leaves nothing behind.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use minimap_core::{attachments::clean_name, export_md};
use minimap_store::{meta, security::RawKey};
use minimap_sync::media::MediaCache;
use minimap_types::{
    AppError, DataExport, ExportAllResult, ExportFormat, ExportManifest, Uuid, EXPORT_FORMAT,
    EXPORT_FORMAT_VERSION,
};
use serde::Serialize;
use tauri::State;
use time::OffsetDateTime;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// Exports everything (archived items, the full history, the attached files that are on this
/// computer) as JSON, and with `JsonAndMarkdown` also as readable Markdown, into a new folder
/// inside `path`. The files are plain text whatever protects the database.
#[tauri::command]
pub async fn export_all(
    state: State<'_, AppState>,
    path: String,
    format: ExportFormat,
) -> Result<ExportAllResult, AppError> {
    let parent = checked_folder(&path)?;
    let (data, schema, key) = state
        .run(|conn| {
            Ok((
                minimap_store::export::collect(conn).map_err(store_error)?,
                minimap_store::schema_version(conn).map_err(store_error)?,
                meta::vault_key(conn).map_err(store_error)?,
            ))
        })
        .await?;
    let cache = state.sync.engine.media_cache();
    tauri::async_runtime::spawn_blocking(move || {
        let files = key.as_ref().map(|key| (&cache, key));
        write_export(&parent, &data, schema, format, files, minimap_store::now())
    })
    .await
    .map_err(|e| app_error("internal", e))?
}

/// The folder to export into: a full path to a folder that exists.
fn checked_folder(path: &str) -> Result<PathBuf, AppError> {
    let invalid = |m: &str| app_error("invalid", m);
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err(invalid("Choose a folder to export to"));
    }
    if !path.is_absolute() {
        return Err(invalid("The export folder must be a full path"));
    }
    if !path.is_dir() {
        return Err(invalid("That folder does not exist"));
    }
    Ok(path.to_path_buf())
}

fn io_error(what: &str, e: impl std::fmt::Display) -> AppError {
    app_error("io", format!("{what}: {e}"))
}

/// `20270303-141500`.
fn stamp(at: OffsetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

/// What has been written so far.
#[derive(Default)]
struct Tally {
    files: u32,
    bytes: u64,
    markdown_files: u32,
    attachment_files: u32,
    attachment_files_missing: u32,
}

impl Tally {
    fn wrote(&mut self, bytes: u64) {
        self.files += 1;
        self.bytes += bytes;
    }
}

fn write_text(dir: &Path, relative: &str, text: &str, tally: &mut Tally) -> Result<(), AppError> {
    let target = dir.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| io_error("Couldn't make a folder", e))?;
    }
    fs::write(&target, text).map_err(|e| io_error(&format!("Couldn't write {relative}"), e))?;
    tally.wrote(text.len() as u64);
    Ok(())
}

fn write_json<T: Serialize>(
    dir: &Path,
    name: &str,
    value: &T,
    tally: &mut Tally,
) -> Result<(), AppError> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| app_error("internal", format!("Couldn't write {name}: {e}")))?;
    text.push('\n');
    write_text(dir, &format!("{name}.json"), &text, tally)
}

/// Writes the attached files that are on this computer, decrypted, to
/// `attachments/<id>/<name>`; returns where each went (for the notes' links).
fn write_attachments(
    dir: &Path,
    data: &DataExport,
    files: Option<(&MediaCache, &RawKey)>,
    tally: &mut Tally,
) -> Result<HashMap<Uuid, String>, AppError> {
    let mut paths = HashMap::new();
    for record in data.attachments.iter().filter(|a| a.archived_at.is_none()) {
        let Some((cache, key)) = files.filter(|(cache, _)| cache.has(&record.sha256)) else {
            tally.attachment_files_missing += 1;
            continue;
        };
        let relative = format!(
            "attachments/{}/{}",
            record.id,
            clean_name(&record.file_name)
        );
        let target = dir.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error("Couldn't make a folder", e))?;
        }
        cache
            .export_plain(key, &record.sha256, &target)
            .map_err(|e| io_error(&format!("Couldn't export {}", record.file_name), e))?;
        tally.wrote(fs::metadata(&target).map_or(0, |m| m.len()));
        tally.attachment_files += 1;
        paths.insert(record.id, relative);
    }
    Ok(paths)
}

fn write_all(
    dir: &Path,
    data: &DataExport,
    schema: u32,
    format: ExportFormat,
    files: Option<(&MediaCache, &RawKey)>,
    at: OffsetDateTime,
) -> Result<Tally, AppError> {
    let mut tally = Tally::default();
    write_json(dir, "objectives", &data.objectives, &mut tally)?;
    write_json(dir, "projects", &data.projects, &mut tally)?;
    write_json(dir, "tasks", &data.tasks, &mut tally)?;
    write_json(dir, "people", &data.people, &mut tally)?;
    write_json(dir, "teams", &data.teams, &mut tally)?;
    write_json(dir, "notes", &data.notes, &mut tally)?;
    write_json(dir, "decisions", &data.decisions, &mut tally)?;
    write_json(dir, "waiting_on", &data.waiting_on, &mut tally)?;
    write_json(dir, "edges", &data.edges, &mut tally)?;
    write_json(dir, "attachments", &data.attachments, &mut tally)?;
    write_json(dir, "activity", &data.activity, &mut tally)?;
    write_json(dir, "task_types", &data.task_types, &mut tally)?;

    let attachment_paths = write_attachments(dir, data, files, &mut tally)?;
    if format.markdown() {
        for file in export_md::markdown(data, &attachment_paths) {
            write_text(dir, &file.path, &file.text, &mut tally)?;
            tally.markdown_files += 1;
        }
    }
    write_text(
        dir,
        "README.md",
        &export_md::readme(format.markdown()),
        &mut tally,
    )?;
    let manifest = ExportManifest {
        format: EXPORT_FORMAT.to_owned(),
        format_version: EXPORT_FORMAT_VERSION,
        app: "Minimap".to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version: schema,
        exported_at: at,
        includes_archived: true,
        markdown: format.markdown(),
        counts: data.counts(),
        attachment_files: tally.attachment_files,
        attachment_files_missing: tally.attachment_files_missing,
    };
    write_json(dir, "manifest", &manifest, &mut tally)?;
    Ok(tally)
}

/// Writes the export into a new `minimap-export-<time>` folder inside `parent`.
pub(crate) fn write_export(
    parent: &Path,
    data: &DataExport,
    schema: u32,
    format: ExportFormat,
    files: Option<(&MediaCache, &RawKey)>,
    at: OffsetDateTime,
) -> Result<ExportAllResult, AppError> {
    let base = format!("minimap-export-{}", stamp(at));
    let mut name = base.clone();
    let mut n = 2;
    while parent.join(&name).exists() || parent.join(format!("{name}.part")).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }
    let part = parent.join(format!("{name}.part"));
    let done = parent.join(&name);
    fs::create_dir(&part).map_err(|e| io_error("Couldn't make the export folder", e))?;
    let built = write_all(&part, data, schema, format, files, at).and_then(|tally| {
        fs::rename(&part, &done).map_err(|e| io_error("Couldn't finish the export", e))?;
        Ok(tally)
    });
    match built {
        Ok(tally) => Ok(ExportAllResult {
            folder: done.display().to_string(),
            files: tally.files,
            bytes: tally.bytes,
            counts: data.counts(),
            markdown_files: tally.markdown_files,
            attachment_files: tally.attachment_files,
            attachment_files_missing: tally.attachment_files_missing,
        }),
        Err(e) => {
            // Nothing half-written is left lying around.
            let _ = fs::remove_dir_all(&part);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeType, UpdateNote};
    use serde::de::DeserializeOwned;
    use time::macros::{date, datetime};

    const AT: OffsetDateTime = datetime!(2027-03-03 14:15:00 UTC);

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "minimap-export-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn demo() -> minimap_store::Connection {
        let mut conn = minimap_store::open_in_memory().unwrap();
        minimap_store::demo::seed(&mut conn, date!(2027 - 03 - 03)).unwrap();
        conn
    }

    fn read<T: DeserializeOwned>(folder: &str, name: &str) -> T {
        let text = fs::read_to_string(Path::new(folder).join(format!("{name}.json"))).unwrap();
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}.json: {e}"))
    }

    fn names_in(folder: &str, sub: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(Path::new(folder).join(sub))
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn the_demo_data_round_trips_through_the_files_into_the_same_structs() {
        let mut conn = demo();
        // Archived items and removed links are part of "everything".
        let task = minimap_store::tasks::list(&conn, false).unwrap().remove(0);
        minimap_store::nodes::archive(&mut conn, NodeRef::new(NodeType::Task, task.id)).unwrap();
        let data = minimap_store::export::collect(&conn).unwrap();
        assert!(data.tasks.iter().any(|t| t.archived_at.is_some()));
        assert!(data.edges.iter().any(|e| e.archived_at.is_some()));

        let out = Dir::new("roundtrip");
        let result = write_export(&out.0, &data, 8, ExportFormat::Json, None, AT).unwrap();
        let folder = &result.folder;
        let back = DataExport {
            objectives: read(folder, "objectives"),
            projects: read(folder, "projects"),
            tasks: read(folder, "tasks"),
            people: read(folder, "people"),
            teams: read(folder, "teams"),
            notes: read(folder, "notes"),
            decisions: read(folder, "decisions"),
            waiting_on: read(folder, "waiting_on"),
            edges: read(folder, "edges"),
            attachments: read(folder, "attachments"),
            activity: read(folder, "activity"),
            task_types: read(folder, "task_types"),
        };
        assert_eq!(back, data, "the files do not read back into the same data");
        assert_eq!(back.tasks.len(), 40);
        assert_eq!(back.people.len(), 8);
        assert!(back.activity.len() > 100);
    }

    #[test]
    fn the_manifest_says_what_the_export_is() {
        let conn = demo();
        let data = minimap_store::export::collect(&conn).unwrap();
        let out = Dir::new("manifest");
        let result = write_export(&out.0, &data, 8, ExportFormat::Json, None, AT).unwrap();
        assert!(result.folder.ends_with("minimap-export-20270303-141500"));
        let manifest: ExportManifest = read(&result.folder, "manifest");
        assert_eq!(manifest.format, "minimap-export");
        assert_eq!(manifest.format_version, 1);
        assert_eq!(manifest.app, "Minimap");
        assert_eq!(manifest.app_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest.schema_version, 8);
        assert_eq!(manifest.exported_at, AT);
        assert!(manifest.includes_archived && !manifest.markdown);
        assert_eq!(manifest.counts, data.counts());
        assert_eq!(manifest.counts["tasks"], 40);
        // The manifest and README are in the folder too, and the result counts every file.
        let all = names_in(&result.folder, "");
        assert!(all.contains(&"README.md".to_owned()));
        assert_eq!(result.files as usize, all.len());
        let size: u64 = all
            .iter()
            .map(|n| {
                fs::metadata(Path::new(&result.folder).join(n))
                    .unwrap()
                    .len()
            })
            .sum();
        assert_eq!(result.bytes, size);
        // Plain JSON only: no Markdown folders.
        assert!(!all.contains(&"projects".to_owned()) && !all.contains(&"notes".to_owned()));
    }

    #[test]
    fn markdown_adds_projects_the_inbox_and_notes() {
        let conn = demo();
        let data = minimap_store::export::collect(&conn).unwrap();
        let out = Dir::new("markdown");
        let result =
            write_export(&out.0, &data, 8, ExportFormat::JsonAndMarkdown, None, AT).unwrap();
        assert_eq!(
            result.markdown_files,
            3 + 1 + 3,
            "3 projects, the inbox, 3 notes"
        );
        assert_eq!(
            names_in(&result.folder, "projects"),
            [
                "eu-region.md",
                "platform-cost-reduction.md",
                "security-and-compliance.md"
            ]
        );
        assert_eq!(names_in(&result.folder, "notes").len(), 3);
        let eu =
            fs::read_to_string(Path::new(&result.folder).join("projects/eu-region.md")).unwrap();
        assert!(eu.starts_with("# EU Region\n"));
        assert!(eu.contains("Provision EU network and clusters"));
        assert!(eu.contains("blocked by"));
        // Notes read on their own: no internal links left.
        for name in names_in(&result.folder, "notes") {
            let text =
                fs::read_to_string(Path::new(&result.folder).join("notes").join(&name)).unwrap();
            assert!(!text.contains("node:"), "{name}: {text}");
        }
        let manifest: ExportManifest = read(&result.folder, "manifest");
        assert!(manifest.markdown);
        assert!(
            fs::read_to_string(Path::new(&result.folder).join("README.md"))
                .unwrap()
                .contains("projects/")
        );
    }

    #[test]
    fn an_export_never_overwrites_an_earlier_one() {
        let conn = demo();
        let data = minimap_store::export::collect(&conn).unwrap();
        let out = Dir::new("twice");
        let a = write_export(&out.0, &data, 8, ExportFormat::Json, None, AT).unwrap();
        let b = write_export(&out.0, &data, 8, ExportFormat::Json, None, AT).unwrap();
        assert_ne!(a.folder, b.folder);
        assert!(b.folder.ends_with("-2"));
        assert_eq!(names_in(&out.0.display().to_string(), "").len(), 2);
        // And nothing else in the chosen folder was touched.
        fs::write(out.0.join("mine.txt"), "keep").unwrap();
        write_export(&out.0, &data, 8, ExportFormat::Json, None, AT).unwrap();
        assert_eq!(fs::read_to_string(out.0.join("mine.txt")).unwrap(), "keep");
    }

    #[test]
    fn only_an_existing_full_path_to_a_folder_is_accepted() {
        let out = Dir::new("paths");
        let file = out.0.join("a-file");
        fs::write(&file, "x").unwrap();
        for (path, why) in [
            ("", "Choose a folder"),
            ("relative/folder", "full path"),
            (out.0.join("missing").to_str().unwrap(), "does not exist"),
            (file.to_str().unwrap(), "does not exist"),
        ] {
            let e = checked_folder(path).unwrap_err();
            assert_eq!(e.code, "invalid", "{path}");
            assert!(e.message.contains(why), "{path}: {}", e.message);
        }
        assert!(checked_folder(out.0.to_str().unwrap()).is_ok());
    }

    /// A cache with one attachment in it, and the row for it.
    fn with_attachment(
        conn: &mut minimap_store::Connection,
        cache_dir: &Path,
        bytes: &[u8],
        name: &str,
    ) -> (MediaCache, RawKey, Uuid) {
        let key = RawKey::generate().unwrap();
        let cache = MediaCache::new(cache_dir);
        let sealed = cache.add_bytes(&key, bytes).unwrap();
        let note = minimap_store::notes::list(conn, false).unwrap().remove(0);
        let row = minimap_store::attachments::add(
            conn,
            NodeRef::new(NodeType::Note, note.id),
            name,
            sealed.plain_bytes,
            &sealed.sha256,
        )
        .unwrap();
        // The note shows the picture.
        minimap_store::notes::update(
            conn,
            note.id,
            UpdateNote {
                body: Some(format!("Look: {}", row.markdown)),
                ..Default::default()
            },
        )
        .unwrap();
        (cache, key, row.id)
    }

    #[test]
    fn attached_files_are_exported_as_they_were_and_notes_link_to_them() {
        let mut conn = demo();
        let cache_dir = Dir::new("cache");
        let (cache, key, id) = with_attachment(
            &mut conn,
            &cache_dir.0,
            b"\x89PNG not really",
            "chart v2.png",
        );
        let data = minimap_store::export::collect(&conn).unwrap();
        let out = Dir::new("attachments");
        let result = write_export(
            &out.0,
            &data,
            8,
            ExportFormat::JsonAndMarkdown,
            Some((&cache, &key)),
            AT,
        )
        .unwrap();
        assert_eq!(
            (result.attachment_files, result.attachment_files_missing),
            (1, 0)
        );
        let exported = Path::new(&result.folder).join(format!("attachments/{id}/chart v2.png"));
        assert_eq!(fs::read(&exported).unwrap(), b"\x89PNG not really");
        let records: Vec<minimap_types::AttachmentRecord> = read(&result.folder, "attachments");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].file_name, "chart v2.png");
        // The note that shows it links to the file, relative to its own folder.
        let notes = names_in(&result.folder, "notes");
        let text: String = notes
            .iter()
            .map(|n| fs::read_to_string(Path::new(&result.folder).join("notes").join(n)).unwrap())
            .collect();
        assert!(
            text.contains(&format!("(../attachments/{id}/chart%20v2.png)")),
            "{text}"
        );
        assert!(!text.contains("attachment:"));
        let manifest: ExportManifest = read(&result.folder, "manifest");
        assert_eq!(manifest.attachment_files, 1);
    }

    #[test]
    fn files_that_are_only_on_drive_are_counted_not_faked() {
        let mut conn = demo();
        let cache_dir = Dir::new("cache-away");
        let (cache, key, _) = with_attachment(&mut conn, &cache_dir.0, b"bytes", "plan.pdf");
        // The file is gone from this computer's cache (it lives on Drive).
        for f in fs::read_dir(&cache_dir.0).unwrap().flatten() {
            fs::remove_file(f.path()).unwrap();
        }
        let data = minimap_store::export::collect(&conn).unwrap();
        let out = Dir::new("away");
        let result = write_export(
            &out.0,
            &data,
            8,
            ExportFormat::Json,
            Some((&cache, &key)),
            AT,
        )
        .unwrap();
        assert_eq!(
            (result.attachment_files, result.attachment_files_missing),
            (0, 1)
        );
        assert!(!Path::new(&result.folder).join("attachments").exists());
        // The row is still in attachments.json, so nothing is hidden.
        let records: Vec<minimap_types::AttachmentRecord> = read(&result.folder, "attachments");
        assert_eq!(records.len(), 1);
        let manifest: ExportManifest = read(&result.folder, "manifest");
        assert_eq!(manifest.attachment_files_missing, 1);
    }

    #[test]
    fn a_failure_part_way_leaves_nothing_behind() {
        let mut conn = demo();
        let cache_dir = Dir::new("cache-broken");
        let (cache, key, _) = with_attachment(&mut conn, &cache_dir.0, b"some bytes", "plan.pdf");
        // Damage the stored file, so decrypting it fails after the JSON was written.
        for f in fs::read_dir(&cache_dir.0).unwrap().flatten() {
            fs::write(f.path(), b"garbage").unwrap();
        }
        let data = minimap_store::export::collect(&conn).unwrap();
        let out = Dir::new("broken");
        let e = write_export(
            &out.0,
            &data,
            8,
            ExportFormat::Json,
            Some((&cache, &key)),
            AT,
        )
        .unwrap_err();
        assert_eq!(e.code, "io");
        assert!(e.message.contains("plan.pdf"), "{}", e.message);
        assert!(
            names_in(&out.0.display().to_string(), "").is_empty(),
            "{:?}",
            names_in(&out.0.display().to_string(), "")
        );
    }

    #[test]
    fn an_empty_database_exports_cleanly() {
        let conn = minimap_store::open_in_memory().unwrap();
        let data = minimap_store::export::collect(&conn).unwrap();
        let out = Dir::new("empty");
        let result =
            write_export(&out.0, &data, 8, ExportFormat::JsonAndMarkdown, None, AT).unwrap();
        assert_eq!(result.markdown_files, 0);
        let tasks: Vec<minimap_types::Task> = read(&result.folder, "tasks");
        assert!(tasks.is_empty());
    }
}
