//! A [`Remote`] on Google Drive (REST API v3). Everything lives in one folder called `Minimap`
//! in the user's Drive; with the `drive.file` scope the app can see only files it made itself.
//!
//! * Uploads use Drive's *resumable* protocol in chunks: the old file stays in place until the
//!   new content has arrived in full, and an interrupted chunk is resumed from where the server
//!   got to rather than starting over.
//! * Listings are paged. Calls that fail in a way worth retrying (no network, 429, 5xx) are
//!   retried with a short back-off; a 401 refreshes the access token once.
//! * Drive allows several files with one name, so lookups take the newest and ignore the rest.

use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::{
    error::{Result, SyncError},
    oauth::TokenSource,
    remote::{safe_name, CheckpointFile, DeviceFile, DeviceMeta, MediaFile, Remote},
};

const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const ROOT_FOLDER: &str = "Minimap";
/// The layout version written into each device file's properties.
const FORMAT: &str = "1";
/// Files up to this size go up in one request.
const SINGLE_REQUEST_BYTES: u64 = 5 * 1024 * 1024;
/// Chunks of a resumable upload (a multiple of 256 KiB, as Drive requires).
pub const CHUNK_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ApiBase {
    /// `https://www.googleapis.com/drive/v3`
    pub files: String,
    /// `https://www.googleapis.com/upload/drive/v3`
    pub upload: String,
}

impl Default for ApiBase {
    fn default() -> Self {
        Self {
            files: "https://www.googleapis.com/drive/v3".into(),
            upload: "https://www.googleapis.com/upload/drive/v3".into(),
        }
    }
}

/// A `reqwest` error as a sync error: trouble reaching Google is `Offline` (retried later).
pub fn network_error(e: &reqwest::Error) -> SyncError {
    if e.is_connect() || e.is_timeout() || e.is_request() || e.is_body() {
        SyncError::Offline("check your internet connection".into())
    } else {
        SyncError::Drive("the connection failed unexpectedly".into())
    }
}

/// The HTTP client used for Google: timeouts so a stalled connection can't hang the app.
pub fn http_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("Minimap/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| SyncError::Io(format!("Couldn't set up the network client: {e}")))
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    created_time: Option<String>,
    #[serde(default)]
    modified_time: Option<String>,
    #[serde(default)]
    app_properties: Option<HashMap<String, String>>,
}

impl DriveFile {
    fn bytes(&self) -> u64 {
        self.size
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
}

/// Escapes a value for a Drive search query (`name = '...'`).
fn q_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Folder {
    Root,
    Devices,
    Checkpoints,
    Media,
}

impl Folder {
    fn name(self) -> &'static str {
        match self {
            Folder::Root => ROOT_FOLDER,
            Folder::Devices => "devices",
            Folder::Checkpoints => "checkpoints",
            Folder::Media => "media",
        }
    }
}

/// Where an upload's content goes.
enum Target<'a> {
    Create {
        parent: &'a str,
        name: &'a str,
        props: Option<HashMap<String, String>>,
    },
    Replace {
        id: &'a str,
        props: Option<HashMap<String, String>>,
    },
}

/// What is being uploaded.
enum Source<'a> {
    File(&'a Path),
    Bytes(&'a [u8]),
}

enum Progress {
    Done(String),
    At(u64),
}

pub struct DriveRemote {
    http: Client,
    tokens: Arc<TokenSource>,
    api: ApiBase,
    folders: Mutex<HashMap<Folder, String>>,
    skew_secs: Mutex<Option<i64>>,
    chunk: usize,
    retry_pause: Duration,
}

impl DriveRemote {
    pub fn new(http: Client, tokens: Arc<TokenSource>, api: ApiBase) -> Self {
        Self {
            http,
            tokens,
            api,
            folders: Mutex::new(HashMap::new()),
            skew_secs: Mutex::new(None),
            chunk: CHUNK_BYTES,
            retry_pause: Duration::from_millis(500),
        }
    }

    /// Smaller chunks and shorter pauses, so tests can cross chunk boundaries quickly.
    #[cfg(test)]
    fn for_tests(mut self, chunk: usize) -> Self {
        self.chunk = chunk;
        self.retry_pause = Duration::from_millis(5);
        self
    }

    fn note_date(&self, response: &Response) {
        let Some(date) = response.headers().get(reqwest::header::DATE) else {
            return;
        };
        let Ok(text) = date.to_str() else { return };
        if let Ok(server) =
            OffsetDateTime::parse(text, &time::format_description::well_known::Rfc2822)
        {
            let skew = (OffsetDateTime::now_utc() - server).whole_seconds();
            *self.skew_secs.lock().unwrap_or_else(|e| e.into_inner()) = Some(skew);
        }
    }

    fn error_from(&self, response: Response) -> SyncError {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        let (message, reason) = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .map(|v| {
                (
                    v["error"]["message"].as_str().unwrap_or("").to_owned(),
                    v["error"]["errors"][0]["reason"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned(),
                )
            })
            .unwrap_or_default();
        match (status.as_u16(), reason.as_str()) {
            (401, _) => SyncError::Auth("Google rejected the sign-in".into()),
            (_, "storageQuotaExceeded") => SyncError::Drive("your Google Drive is full".into()),
            (403, "insufficientPermissions" | "forbidden") => SyncError::Auth(
                "Google didn't allow Minimap to use Drive. Connect again from Settings".into(),
            ),
            (403, "accessNotConfigured" | "SERVICE_DISABLED") => SyncError::Drive(
                "the Google Drive API isn't turned on for the OAuth client's project".into(),
            ),
            (s, _) if s >= 500 || s == 429 => {
                SyncError::Drive(format!("a temporary problem ({s}); try again"))
            }
            (s, _) if message.is_empty() => SyncError::Drive(format!("error {s}")),
            (s, _) => SyncError::Drive(format!("{message} ({s})")),
        }
    }

    /// Sends a request built by `make` (given the HTTP client and a bearer token), retrying what
    /// is worth retrying. Statuses 2xx, 308 and 404 are handed back for the caller to read.
    fn send(&self, make: &dyn Fn(&Client, &str) -> RequestBuilder) -> Result<Response> {
        self.send_with(make, 5)
    }

    /// [`send`](Self::send) with a limit on attempts. A chunk of a resumable upload is sent
    /// once: if the answer is lost the server may have kept it, so the caller asks where the
    /// upload stands instead of sending the same bytes again.
    fn send_with(
        &self,
        make: &dyn Fn(&Client, &str) -> RequestBuilder,
        attempts: u32,
    ) -> Result<Response> {
        let mut token = self.tokens.access_token()?;
        let mut refreshed = false;
        let mut last: Option<SyncError> = None;
        let mut attempt = 0u32;
        while attempt < attempts {
            if attempt > 0 {
                std::thread::sleep(self.retry_pause * (1 << (attempt - 1)));
            }
            match make(&self.http, &token).send() {
                Ok(response) => {
                    self.note_date(&response);
                    let status = response.status();
                    let code = status.as_u16();
                    if status.is_success() || code == 308 || code == 404 {
                        return Ok(response);
                    }
                    if code == 401 && !refreshed {
                        // A new token is not a retry: it doesn't use up an attempt.
                        token = self.tokens.refresh()?;
                        refreshed = true;
                        continue;
                    }
                    let transient = code == 429 || status.is_server_error();
                    let error = self.error_from(response);
                    if !transient {
                        return Err(error);
                    }
                    last = Some(error);
                }
                Err(e) => {
                    let error = network_error(&e);
                    if !matches!(error, SyncError::Offline(_)) {
                        return Err(error);
                    }
                    last = Some(error);
                }
            }
            attempt += 1;
        }
        Err(last.unwrap_or_else(|| SyncError::Drive("too many failed attempts".into())))
    }

    fn json<T: for<'de> Deserialize<'de>>(&self, response: Response) -> Result<T> {
        response
            .json()
            .map_err(|_| SyncError::Drive("Google sent a reply Minimap doesn't understand".into()))
    }

    fn ok(&self, response: Response) -> Result<Response> {
        if response.status().as_u16() == 404 {
            return Err(SyncError::Drive("that file isn't on Google Drive".into()));
        }
        Ok(response)
    }

    // ------------------------------------------------------------ finding things

    fn list(&self, query: &str) -> Result<Vec<DriveFile>> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let response = self.send(&|http, token| {
                let mut req = http
                    .get(format!("{}/files", self.api.files))
                    .bearer_auth(token)
                    .query(&[
                        ("q", query),
                        ("fields", "nextPageToken,files(id,name,size,createdTime,modifiedTime,appProperties)"),
                        ("pageSize", "1000"),
                        ("orderBy", "modifiedTime desc"),
                        ("spaces", "drive"),
                    ]);
                if let Some(p) = &page {
                    req = req.query(&[("pageToken", p.as_str())]);
                }
                req
            })?;
            let list: FileList = self.json(self.ok(response)?)?;
            out.extend(list.files);
            match list.next_page_token {
                Some(next) if !next.is_empty() => page = Some(next),
                _ => return Ok(out),
            }
        }
    }

    fn children(&self, parent: &str) -> Result<Vec<DriveFile>> {
        self.list(&format!(
            "'{}' in parents and trashed = false and mimeType != '{FOLDER_MIME}'",
            q_escape(parent)
        ))
    }

    fn find_child(&self, parent: &str, name: &str) -> Result<Option<DriveFile>> {
        Ok(self
            .list(&format!(
                "name = '{}' and '{}' in parents and trashed = false and mimeType != '{FOLDER_MIME}'",
                q_escape(name),
                q_escape(parent)
            ))?
            .into_iter()
            .next())
    }

    /// The id of one of Minimap's folders. With `create`, a missing folder is made; without,
    /// a missing one is `None` (nothing has been saved there yet).
    fn folder(&self, which: Folder, create: bool) -> Result<Option<String>> {
        if let Some(id) = self
            .folders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&which)
        {
            return Ok(Some(id.clone()));
        }
        let parent = match which {
            Folder::Root => "root".to_owned(),
            _ => match self.folder(Folder::Root, create)? {
                Some(id) => id,
                None => return Ok(None),
            },
        };
        let found = self
            .list(&format!(
                "name = '{}' and '{}' in parents and trashed = false and mimeType = '{FOLDER_MIME}'",
                q_escape(which.name()),
                q_escape(&parent)
            ))?
            .into_iter()
            .last(); // the oldest: two devices racing to make it agree on the first
        let id = match found {
            Some(f) => f.id,
            None if create => {
                let body = serde_json::json!({
                    "name": which.name(),
                    "mimeType": FOLDER_MIME,
                    "parents": [parent],
                });
                let response = self.send(&|http, token| {
                    http.post(format!("{}/files", self.api.files))
                        .bearer_auth(token)
                        .query(&[("fields", "id")])
                        .json(&body)
                })?;
                let made: DriveFile = self.json(self.ok(response)?)?;
                made.id
            }
            None => return Ok(None),
        };
        self.folders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(which, id.clone());
        Ok(Some(id))
    }

    // ------------------------------------------------------------ transfer

    fn download(&self, id: &str, to: &Path) -> Result<()> {
        let response = self.send(&|http, token| {
            http.get(format!("{}/files/{id}", self.api.files))
                .bearer_auth(token)
                .query(&[("alt", "media")])
                .timeout(Duration::from_secs(900))
        })?;
        let mut response = self.ok(response)?;
        let mut file =
            fs::File::create(to).map_err(|e| SyncError::io("Couldn't write the download", e))?;
        let copied = response.copy_to(&mut file);
        if let Err(e) = copied {
            drop(file);
            let _ = fs::remove_file(to);
            return Err(network_error(&e));
        }
        file.flush()
            .map_err(|e| SyncError::io("Couldn't write the download", e))
    }

    fn download_bytes(&self, id: &str) -> Result<Vec<u8>> {
        let response = self.send(&|http, token| {
            http.get(format!("{}/files/{id}", self.api.files))
                .bearer_auth(token)
                .query(&[("alt", "media")])
        })?;
        let bytes = self.ok(response)?.bytes().map_err(|e| network_error(&e))?;
        Ok(bytes.to_vec())
    }

    fn delete_file(&self, id: &str) -> Result<()> {
        let response = self.send(&|http, token| {
            http.delete(format!("{}/files/{id}", self.api.files))
                .bearer_auth(token)
        })?;
        let _ = response; // 404 means it was already gone
        Ok(())
    }

    fn read_chunk(source: &Source, offset: u64, len: usize) -> Result<Vec<u8>> {
        match source {
            Source::Bytes(b) => {
                let start = usize::try_from(offset).unwrap_or(usize::MAX).min(b.len());
                Ok(b[start..(start + len).min(b.len())].to_vec())
            }
            Source::File(path) => {
                let mut f =
                    fs::File::open(path).map_err(|e| SyncError::io("Couldn't read the file", e))?;
                f.seek(SeekFrom::Start(offset))
                    .map_err(|e| SyncError::io("Couldn't read the file", e))?;
                let mut buf = vec![0u8; len];
                f.read_exact(&mut buf)
                    .map_err(|e| SyncError::io("Couldn't read the file", e))?;
                Ok(buf)
            }
        }
    }

    /// Uploads `source` to `target` with the resumable protocol and returns the file's id.
    fn upload(&self, target: Target, source: Source) -> Result<String> {
        let size = match &source {
            Source::Bytes(b) => b.len() as u64,
            Source::File(p) => fs::metadata(p)
                .map_err(|e| SyncError::io("Couldn't read the file", e))?
                .len(),
        };
        if size == 0 {
            return Err(SyncError::Invalid("There is nothing to upload".into()));
        }
        let session = self.start_session(&target, size)?;
        let mut offset = 0u64;
        let mut failures = 0u32;
        loop {
            let len = if size <= SINGLE_REQUEST_BYTES {
                usize::try_from(size - offset).unwrap_or(usize::MAX)
            } else {
                self.chunk
                    .min(usize::try_from(size - offset).unwrap_or(usize::MAX))
            };
            let body = Self::read_chunk(&source, offset, len)?;
            let range = format!("bytes {}-{}/{size}", offset, offset + len as u64 - 1);
            match self.put_chunk(&session, &range, body) {
                Ok(Progress::Done(id)) => return Ok(id),
                Ok(Progress::At(next)) => {
                    offset = next;
                    failures = 0;
                    if offset >= size {
                        // The server has every byte but didn't say "done": ask.
                        if let Progress::Done(id) = self.query_progress(&session, size)? {
                            return Ok(id);
                        }
                        return Err(SyncError::Drive("the upload didn't finish".into()));
                    }
                }
                Err(e) if e.is_transient() || matches!(e, SyncError::Offline(_)) => {
                    failures += 1;
                    if failures > 5 {
                        return Err(e);
                    }
                    std::thread::sleep(self.retry_pause * failures);
                    // Where did the server get to? (It may have kept the chunk.)
                    match self.query_progress(&session, size) {
                        Ok(Progress::Done(id)) => return Ok(id),
                        Ok(Progress::At(next)) => offset = next,
                        Err(_) => {}
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn start_session(&self, target: &Target, size: u64) -> Result<String> {
        let (method_post, url, metadata) = match target {
            Target::Create {
                parent,
                name,
                props,
            } => {
                let mut meta = serde_json::json!({ "name": name, "parents": [parent] });
                if let Some(p) = props {
                    meta["appProperties"] = serde_json::json!(p);
                }
                (true, format!("{}/files", self.api.upload), meta)
            }
            Target::Replace { id, props } => {
                let mut meta = serde_json::json!({});
                if let Some(p) = props {
                    meta["appProperties"] = serde_json::json!(p);
                }
                (false, format!("{}/files/{id}", self.api.upload), meta)
            }
        };
        let response = self.send(&|http, token| {
            let req = if method_post {
                http.post(&url)
            } else {
                http.patch(&url)
            };
            req.bearer_auth(token)
                .query(&[("uploadType", "resumable"), ("fields", "id")])
                .header("X-Upload-Content-Type", "application/octet-stream")
                .header("X-Upload-Content-Length", size.to_string())
                .json(&metadata)
        })?;
        let response = self.ok(response)?;
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
            .ok_or_else(|| SyncError::Drive("Google didn't start the upload".into()))
    }

    fn put_chunk(&self, session: &str, range: &str, body: Vec<u8>) -> Result<Progress> {
        let response = self.send_with(
            &|http, token| {
                http.put(session)
                    .bearer_auth(token)
                    .header("Content-Range", range)
                    .timeout(Duration::from_secs(600))
                    .body(body.clone())
            },
            // One retry only for the sign-in refresh; everything else is for the caller to
            // sort out by asking the server how much it has.
            1,
        )?;
        self.progress_of(response)
    }

    fn query_progress(&self, session: &str, size: u64) -> Result<Progress> {
        let range = format!("bytes */{size}");
        let response = self.send(&|http, token| {
            http.put(session)
                .bearer_auth(token)
                .header("Content-Range", range.as_str())
                .header(reqwest::header::CONTENT_LENGTH, "0")
        })?;
        self.progress_of(response)
    }

    fn progress_of(&self, response: Response) -> Result<Progress> {
        let code = response.status().as_u16();
        match code {
            200 | 201 => {
                let made: DriveFile = self.json(response)?;
                Ok(Progress::Done(made.id))
            }
            308 => {
                // "bytes=0-N": the server has bytes 0..=N. No header: it has nothing yet.
                let next = response
                    .headers()
                    .get(reqwest::header::RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.rsplit('-').next())
                    .and_then(|n| n.parse::<u64>().ok())
                    .map_or(0, |last| last + 1);
                Ok(Progress::At(next))
            }
            404 | 410 => Err(SyncError::Drive(
                "the upload session expired; try again".into(),
            )),
            _ => Err(self.error_from(response)),
        }
    }
}

fn props(pairs: &[(&str, String)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

impl Remote for DriveRemote {
    fn account(&self) -> Result<String> {
        let response = self.send(&|http, token| {
            http.get(format!("{}/about", self.api.files))
                .bearer_auth(token)
                .query(&[("fields", "user(emailAddress,displayName)")])
        })?;
        let about: serde_json::Value = self.json(self.ok(response)?)?;
        Ok(about["user"]["emailAddress"]
            .as_str()
            .or_else(|| about["user"]["displayName"].as_str())
            .unwrap_or("Google account")
            .to_owned())
    }

    fn clock_skew_seconds(&self) -> Option<i64> {
        *self.skew_secs.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn revoke(&self) {
        self.tokens.revoke();
    }

    fn read_vault(&self) -> Result<Option<Vec<u8>>> {
        let Some(root) = self.folder(Folder::Root, false)? else {
            return Ok(None);
        };
        match self.find_child(&root, "vault.json")? {
            Some(f) => Ok(Some(self.download_bytes(&f.id)?)),
            None => Ok(None),
        }
    }

    fn write_vault(&self, bytes: &[u8]) -> Result<()> {
        let root = self
            .folder(Folder::Root, true)?
            .ok_or_else(|| SyncError::Drive("couldn't create the Minimap folder".into()))?;
        match self.find_child(&root, "vault.json")? {
            Some(f) => self.upload(
                Target::Replace {
                    id: &f.id,
                    props: None,
                },
                Source::Bytes(bytes),
            )?,
            None => self.upload(
                Target::Create {
                    parent: &root,
                    name: "vault.json",
                    props: None,
                },
                Source::Bytes(bytes),
            )?,
        };
        Ok(())
    }

    fn list_devices(&self) -> Result<Vec<DeviceFile>> {
        let Some(devices) = self.folder(Folder::Devices, false)? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for f in self.children(&devices)? {
            if minimap_core::sync::parse_device_file_name(&f.name).is_none() {
                continue;
            }
            let Some(p) = &f.app_properties else { continue };
            let (Some(id), Some(saved), Some(seq)) = (
                p.get("device_id"),
                p.get("saved_at"),
                p.get("seq").and_then(|s| s.parse().ok()),
            ) else {
                continue;
            };
            out.push(DeviceFile {
                device_id: id.clone(),
                device_name: p.get("device_name").cloned().unwrap_or_default(),
                saved_at: saved.clone(),
                seq,
                bytes: f.bytes(),
            });
        }
        Ok(out)
    }

    fn download_device(&self, device_id: &str, to: &Path) -> Result<()> {
        let devices = self
            .folder(Folder::Devices, false)?
            .ok_or_else(|| SyncError::Drive("there are no devices on this Drive yet".into()))?;
        let name = minimap_core::sync::device_file_name(device_id);
        let file = self
            .find_child(&devices, &name)?
            .ok_or_else(|| SyncError::Drive("that device has no saved data".into()))?;
        self.download(&file.id, to)
    }

    fn upload_device(&self, meta: &DeviceMeta, from: &Path) -> Result<()> {
        let devices = self
            .folder(Folder::Devices, true)?
            .ok_or_else(|| SyncError::Drive("couldn't create the devices folder".into()))?;
        let name = minimap_core::sync::device_file_name(&meta.device_id);
        let properties = props(&[
            ("device_id", meta.device_id.clone()),
            ("device_name", meta.device_name.clone()),
            ("saved_at", meta.saved_at.clone()),
            ("seq", meta.seq.to_string()),
            ("format", FORMAT.to_owned()),
        ]);
        let existing = self.find_child(&devices, &name)?;
        match existing {
            Some(f) => self.upload(
                Target::Replace {
                    id: &f.id,
                    props: Some(properties),
                },
                Source::File(from),
            )?,
            None => self.upload(
                Target::Create {
                    parent: &devices,
                    name: &name,
                    props: Some(properties),
                },
                Source::File(from),
            )?,
        };
        Ok(())
    }

    fn list_checkpoints(&self) -> Result<Vec<CheckpointFile>> {
        let Some(folder) = self.folder(Folder::Checkpoints, false)? else {
            return Ok(Vec::new());
        };
        Ok(self
            .children(&folder)?
            .into_iter()
            .filter(|f| minimap_core::sync::parse_checkpoint_name(&f.name).is_some())
            .map(|f| CheckpointFile {
                bytes: f.bytes(),
                name: f.name,
            })
            .collect())
    }

    fn upload_checkpoint(&self, name: &str, from: &Path) -> Result<()> {
        if !safe_name(name) {
            return Err(SyncError::Invalid(format!("{name:?} is not a valid name")));
        }
        let folder = self
            .folder(Folder::Checkpoints, true)?
            .ok_or_else(|| SyncError::Drive("couldn't create the checkpoints folder".into()))?;
        if self.find_child(&folder, name)?.is_some() {
            return Ok(());
        }
        self.upload(
            Target::Create {
                parent: &folder,
                name,
                props: None,
            },
            Source::File(from),
        )?;
        Ok(())
    }

    fn download_checkpoint(&self, name: &str, to: &Path) -> Result<()> {
        if !safe_name(name) {
            return Err(SyncError::Invalid(format!("{name:?} is not a valid name")));
        }
        let folder = self
            .folder(Folder::Checkpoints, false)?
            .ok_or_else(|| SyncError::Drive("there are no checkpoints on this Drive".into()))?;
        let file = self
            .find_child(&folder, name)?
            .ok_or_else(|| SyncError::Drive("that checkpoint is gone".into()))?;
        self.download(&file.id, to)
    }

    fn delete_checkpoint(&self, name: &str) -> Result<()> {
        if !safe_name(name) {
            return Err(SyncError::Invalid(format!("{name:?} is not a valid name")));
        }
        let Some(folder) = self.folder(Folder::Checkpoints, false)? else {
            return Ok(());
        };
        // Every file of that name (Drive can hold duplicates), in this folder only.
        for f in self.list(&format!(
            "name = '{}' and '{}' in parents and trashed = false",
            q_escape(name),
            q_escape(&folder)
        ))? {
            self.delete_file(&f.id)?;
        }
        Ok(())
    }

    fn list_media(&self) -> Result<Vec<MediaFile>> {
        let Some(folder) = self.folder(Folder::Media, false)? else {
            return Ok(Vec::new());
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for f in self.children(&folder)? {
            let Some(sha) = f.name.strip_suffix(".bin") else {
                continue;
            };
            if !minimap_core::attachments::is_sha256(sha) || !seen.insert(sha.to_owned()) {
                continue;
            }
            out.push(MediaFile {
                sha256: sha.to_owned(),
                bytes: f.bytes(),
                created: f.created_time.or(f.modified_time).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    fn upload_media(&self, sha256: &str, from: &Path) -> Result<()> {
        let name = format!("{sha256}.bin");
        if !safe_name(&name) {
            return Err(SyncError::Invalid("not a valid file name".into()));
        }
        let folder = self
            .folder(Folder::Media, true)?
            .ok_or_else(|| SyncError::Drive("couldn't create the media folder".into()))?;
        if self.find_child(&folder, &name)?.is_some() {
            return Ok(());
        }
        self.upload(
            Target::Create {
                parent: &folder,
                name: &name,
                props: None,
            },
            Source::File(from),
        )?;
        Ok(())
    }

    fn download_media(&self, sha256: &str, to: &Path) -> Result<()> {
        let name = format!("{sha256}.bin");
        if !safe_name(&name) {
            return Err(SyncError::Invalid("not a valid file name".into()));
        }
        let folder = self
            .folder(Folder::Media, false)?
            .ok_or_else(|| SyncError::Drive("no attachments are stored on this Drive".into()))?;
        let file = self
            .find_child(&folder, &name)?
            .ok_or_else(|| SyncError::Drive("that attachment isn't on Google Drive".into()))?;
        self.download(&file.id, to)
    }

    fn delete_media(&self, sha256: &str) -> Result<()> {
        let name = format!("{sha256}.bin");
        if !safe_name(&name) {
            return Err(SyncError::Invalid("not a valid file name".into()));
        }
        let Some(folder) = self.folder(Folder::Media, false)? else {
            return Ok(());
        };
        for f in self.list(&format!(
            "name = '{}' and '{}' in parents and trashed = false",
            q_escape(&name),
            q_escape(&folder)
        ))? {
            self.delete_file(&f.id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "drive_tests.rs"]
mod tests;
