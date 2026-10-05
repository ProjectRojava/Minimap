//! The Drive client against a small stand-in for Google's servers running on localhost: the
//! same HTTP calls, headers and paging, none of Google's data.

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

use super::*;
use crate::{
    folder::temp_dir,
    oauth::{ClientCredentials, Endpoints},
};

struct MFile {
    id: String,
    name: String,
    parent: Option<String>,
    folder: bool,
    content: Vec<u8>,
    props: HashMap<String, String>,
    created: String,
    modified: String,
}

enum SessionTarget {
    Create {
        parent: String,
        name: String,
        props: HashMap<String, String>,
    },
    Replace {
        id: String,
        props: HashMap<String, String>,
    },
}

struct Session {
    target: SessionTarget,
    buf: Vec<u8>,
    total: u64,
}

#[derive(Default)]
struct State {
    files: Vec<MFile>,
    next: u32,
    sessions: HashMap<String, Session>,
    log: Vec<String>,
    token_calls: u32,
    token_forms: Vec<HashMap<String, String>>,
    /// The next N API requests answer with this status and body.
    fail_next: Vec<(u16, String)>,
    /// The next N chunk uploads are stored but the connection is dropped before the reply.
    drop_after_store: u32,
    page_cap: usize,
    /// The first API request is refused with 401 (an expired access token).
    expire_first_token: bool,
    date_offset_secs: i64,
    chunk_ranges: Vec<String>,
}

struct Mock {
    base: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct Req {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Ok(v) =
                    u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16)
                {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_pairs(s: &str) -> HashMap<String, String> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

fn read_request(stream: &mut TcpStream) -> Option<Req> {
    let mut data = Vec::new();
    let mut buf = [0u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
    };
    let head = String::from_utf8_lossy(&data[..header_end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_owned();
    let target = first.next()?.to_owned();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_owned()))
        .collect();
    let want: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = data[header_end + 4..].to_vec();
    while body.len() < want {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    Some(Req {
        method,
        path: path.to_owned(),
        query: parse_pairs(query),
        headers,
        body,
    })
}

fn reply(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(&str, String)],
    body: &[u8],
    date_offset: i64,
) {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        308 => "Resume Incomplete",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Status",
    };
    let date = (OffsetDateTime::now_utc() + time::Duration::seconds(date_offset))
        .format(&time::format_description::well_known::Rfc2822)
        .unwrap_or_default();
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\nDate: {date}\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn json(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}

fn stamp(n: u32) -> String {
    let t = time::macros::datetime!(2027-01-01 00:00 UTC) + time::Duration::seconds(i64::from(n));
    minimap_types::timefmt::fmt_ts(t)
}

fn file_json(f: &MFile) -> serde_json::Value {
    serde_json::json!({
        "id": f.id,
        "name": f.name,
        "size": f.content.len().to_string(),
        "createdTime": f.created,
        "modifiedTime": f.modified,
        "appProperties": f.props,
    })
}

fn unescape(s: &str) -> String {
    s.replace("\\'", "'").replace("\\\\", "\\")
}

/// Evaluates the small subset of Drive's query language Minimap uses.
fn matches_query(f: &MFile, q: &str) -> bool {
    q.split(" and ").all(|cond| {
        let cond = cond.trim();
        if let Some(rest) = cond.strip_prefix("name = '") {
            return f.name == unescape(rest.trim_end_matches('\''));
        }
        if let Some(rest) = cond.strip_prefix('\'') {
            if let Some(parent) = rest.strip_suffix("' in parents") {
                let parent = unescape(parent);
                return if parent == "root" {
                    f.parent.is_none()
                } else {
                    f.parent.as_deref() == Some(parent.as_str())
                };
            }
        }
        if let Some(rest) = cond.strip_prefix("mimeType = '") {
            return f.folder == (rest.trim_end_matches('\'') == FOLDER_MIME);
        }
        if let Some(rest) = cond.strip_prefix("mimeType != '") {
            return f.folder != (rest.trim_end_matches('\'') == FOLDER_MIME);
        }
        cond == "trashed = false"
    })
}

fn handle(state: &Arc<Mutex<State>>, base: &str, mut stream: TcpStream) {
    let Some(req) = read_request(&mut stream) else {
        return;
    };
    let mut st = state.lock().unwrap();
    st.log.push(format!("{} {}", req.method, req.path));
    let off = st.date_offset_secs;

    if req.path == "/token" {
        st.token_calls += 1;
        let form = parse_pairs(&String::from_utf8_lossy(&req.body));
        let ok = form
            .get("grant_type")
            .is_some_and(|g| g == "authorization_code" || g == "refresh_token");
        st.token_forms.push(form.clone());
        if !ok {
            return reply(
                &mut stream,
                400,
                &[],
                &json(serde_json::json!({"error":"invalid_request"})),
                off,
            );
        }
        if form.get("refresh_token").is_some_and(|t| t == "revoked") {
            return reply(
                &mut stream,
                400,
                &[],
                &json(
                    serde_json::json!({"error":"invalid_grant","error_description":"Token has been expired or revoked."}),
                ),
                off,
            );
        }
        let mut answer = serde_json::json!({"access_token": format!("access-{}", st.token_calls), "expires_in": 3600});
        if form
            .get("grant_type")
            .is_some_and(|g| g == "authorization_code")
        {
            answer["refresh_token"] = serde_json::json!("refresh-1");
        }
        return reply(&mut stream, 200, &[], &json(answer), off);
    }
    if req.path == "/revoke" {
        return reply(&mut stream, 200, &[], b"{}", off);
    }

    // Everything below is the API: it needs a bearer token.
    if !req
        .headers
        .get("authorization")
        .is_some_and(|a| a.starts_with("Bearer access-"))
    {
        return reply(&mut stream, 401, &[], b"{}", off);
    }
    if st.expire_first_token {
        st.expire_first_token = false;
        return reply(
            &mut stream,
            401,
            &[],
            &json(serde_json::json!({"error":{"message":"Invalid Credentials"}})),
            off,
        );
    }
    if !st.fail_next.is_empty() {
        let (code, body) = st.fail_next.remove(0);
        return reply(&mut stream, code, &[], body.as_bytes(), off);
    }

    let id_from = |prefix: &str| req.path.strip_prefix(prefix).map(str::to_owned);
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/drive/v3/about") => reply(
            &mut stream,
            200,
            &[],
            &json(serde_json::json!({"user":{"emailAddress":"me@example.com"}})),
            off,
        ),
        ("GET", "/drive/v3/files") => {
            let q = req.query.get("q").cloned().unwrap_or_default();
            let mut hits: Vec<&MFile> = st.files.iter().filter(|f| matches_query(f, &q)).collect();
            hits.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.id.cmp(&a.id)));
            let cap = st.page_cap.max(1);
            let start: usize = req
                .query
                .get("pageToken")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            let page: Vec<serde_json::Value> = hits
                .iter()
                .skip(start)
                .take(cap)
                .map(|f| file_json(f))
                .collect();
            let mut answer = serde_json::json!({ "files": page });
            if start + cap < hits.len() {
                answer["nextPageToken"] = serde_json::json!((start + cap).to_string());
            }
            reply(&mut stream, 200, &[], &json(answer), off)
        }
        ("POST", "/drive/v3/files") => {
            let meta: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            st.next += 1;
            let n = st.next;
            let parent = meta["parents"][0]
                .as_str()
                .filter(|p| *p != "root")
                .map(str::to_owned);
            st.files.push(MFile {
                id: format!("id{n}"),
                name: meta["name"].as_str().unwrap_or("").to_owned(),
                parent,
                folder: meta["mimeType"] == FOLDER_MIME,
                content: Vec::new(),
                props: HashMap::new(),
                created: stamp(n),
                modified: stamp(n),
            });
            reply(
                &mut stream,
                200,
                &[],
                &json(serde_json::json!({"id": format!("id{n}")})),
                off,
            )
        }
        ("POST" | "PATCH", path) if path.starts_with("/upload/drive/v3/files") => {
            let meta: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
            let total: u64 = req
                .headers
                .get("x-upload-content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let props: HashMap<String, String> = meta["appProperties"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned()))
                        .collect()
                })
                .unwrap_or_default();
            let target = if req.method == "POST" {
                SessionTarget::Create {
                    parent: meta["parents"][0].as_str().unwrap_or("").to_owned(),
                    name: meta["name"].as_str().unwrap_or("").to_owned(),
                    props,
                }
            } else {
                SessionTarget::Replace {
                    id: path.rsplit('/').next().unwrap_or("").to_owned(),
                    props,
                }
            };
            st.next += 1;
            let session = format!("s{}", st.next);
            st.sessions.insert(
                session.clone(),
                Session {
                    target,
                    buf: Vec::new(),
                    total,
                },
            );
            let location = format!("{base}/upload/session/{session}");
            reply(&mut stream, 200, &[("Location", location)], b"{}", off)
        }
        ("PUT", path) if path.starts_with("/upload/session/") => {
            let key = path.rsplit('/').next().unwrap_or("").to_owned();
            let range = req
                .headers
                .get("content-range")
                .cloned()
                .unwrap_or_default();
            let Some(session) = st.sessions.get(&key) else {
                return reply(&mut stream, 404, &[], b"{}", off);
            };
            let have = session.buf.len() as u64;
            let total = session.total;
            if range.starts_with("bytes */") {
                if have == total && total > 0 {
                    let id = finish(&mut st, &key);
                    return reply(
                        &mut stream,
                        200,
                        &[],
                        &json(serde_json::json!({"id": id})),
                        off,
                    );
                }
                let hdr: Vec<(&str, String)> = if have > 0 {
                    vec![("Range", format!("bytes=0-{}", have - 1))]
                } else {
                    vec![]
                };
                return reply(&mut stream, 308, &hdr, b"", off);
            }
            let spec = range.strip_prefix("bytes ").unwrap_or("");
            let (span, _) = spec.split_once('/').unwrap_or((spec, ""));
            let (from, to) = span.split_once('-').unwrap_or(("0", "0"));
            let (from, to): (u64, u64) = (from.parse().unwrap_or(0), to.parse().unwrap_or(0));
            if from != have || to + 1 - from != req.body.len() as u64 {
                return reply(
                    &mut stream,
                    400,
                    &[],
                    &json(serde_json::json!({"error":{"message":"bad range"}})),
                    off,
                );
            }
            st.chunk_ranges.push(range.clone());
            let drop_it = st.drop_after_store > 0;
            if let Some(s) = st.sessions.get_mut(&key) {
                s.buf.extend_from_slice(&req.body);
            }
            if drop_it {
                st.drop_after_store -= 1;
                return; // the connection closes without a reply
            }
            let now_have = have + req.body.len() as u64;
            if now_have == total {
                let id = finish(&mut st, &key);
                return reply(
                    &mut stream,
                    200,
                    &[],
                    &json(serde_json::json!({"id": id})),
                    off,
                );
            }
            reply(
                &mut stream,
                308,
                &[("Range", format!("bytes=0-{}", now_have - 1))],
                b"",
                off,
            )
        }
        ("GET", path) if path.starts_with("/drive/v3/files/") => {
            let id = id_from("/drive/v3/files/").unwrap_or_default();
            match st.files.iter().find(|f| f.id == id) {
                Some(f) => reply(&mut stream, 200, &[], &f.content, off),
                None => reply(
                    &mut stream,
                    404,
                    &[],
                    &json(serde_json::json!({"error":{"message":"File not found"}})),
                    off,
                ),
            }
        }
        ("DELETE", path) if path.starts_with("/drive/v3/files/") => {
            let id = id_from("/drive/v3/files/").unwrap_or_default();
            let before = st.files.len();
            st.files.retain(|f| f.id != id);
            let code = if st.files.len() < before { 204 } else { 404 };
            reply(&mut stream, code, &[], b"", off)
        }
        _ => reply(&mut stream, 404, &[], b"{}", off),
    }
}

/// Completes an upload session: creates or replaces the file. Replacing keeps the old content
/// until this moment, like Drive.
fn finish(st: &mut State, key: &str) -> String {
    let Some(session) = st.sessions.remove(key) else {
        return String::new();
    };
    st.next += 1;
    let n = st.next;
    match session.target {
        SessionTarget::Create {
            parent,
            name,
            props,
        } => {
            let id = format!("id{n}");
            st.files.push(MFile {
                id: id.clone(),
                name,
                parent: Some(parent),
                folder: false,
                content: session.buf,
                props,
                created: stamp(n),
                modified: stamp(n),
            });
            id
        }
        SessionTarget::Replace { id, props } => {
            if let Some(f) = st.files.iter_mut().find(|f| f.id == id) {
                f.content = session.buf;
                if !props.is_empty() {
                    f.props = props;
                }
                f.modified = stamp(n);
            }
            id
        }
    }
}

fn start_mock() -> Mock {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let state = Arc::new(Mutex::new(State {
        page_cap: 1000,
        ..Default::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = {
        let (state, stop, base) = (state.clone(), stop.clone(), base.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let (state, base) = (state.clone(), base.clone());
                        std::thread::spawn(move || handle(&state, &base, stream));
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
        })
    };
    Mock {
        base,
        state,
        stop,
        handle: Some(handle),
    }
}

fn creds() -> ClientCredentials {
    ClientCredentials {
        client_id: "client-id".into(),
        client_secret: "client-secret".into(),
    }
}

fn endpoints(mock: &Mock) -> Endpoints {
    Endpoints {
        auth: format!("{}/auth", mock.base),
        token: format!("{}/token", mock.base),
        revoke: format!("{}/revoke", mock.base),
    }
}

fn remote(mock: &Mock, refresh: &str, chunk: usize) -> DriveRemote {
    let http = http_client().unwrap();
    let tokens = Arc::new(TokenSource::new(
        http.clone(),
        endpoints(mock),
        creds(),
        refresh.into(),
    ));
    let api = ApiBase {
        files: format!("{}/drive/v3", mock.base),
        upload: format!("{}/upload/drive/v3", mock.base),
    };
    DriveRemote::new(http, tokens, api).for_tests(chunk)
}

fn requests(mock: &Mock, prefix: &str) -> usize {
    mock.state
        .lock()
        .unwrap()
        .log
        .iter()
        .filter(|l| l.contains(prefix))
        .count()
}

#[test]
fn the_drive_client_meets_the_same_contract_as_every_remote() {
    let mock = start_mock();
    let dir = temp_dir("drive-contract");
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    crate::remote::contract(&drive, &dir.join("scratch"));
    // Minimap made its folders under one `Minimap` folder, and nothing else.
    let st = mock.state.lock().unwrap();
    let folders: Vec<&str> = st
        .files
        .iter()
        .filter(|f| f.folder)
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(folders, vec!["Minimap", "devices", "checkpoints", "media"]);
    drop(st);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn big_files_go_up_in_chunks_and_replacing_keeps_the_old_file_until_complete() {
    let mock = start_mock();
    let dir = temp_dir("drive-chunks");
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    let big: Vec<u8> = (0..6 * 1024 * 1024 + 123)
        .map(|i| (i % 253) as u8)
        .collect();
    let file = dir.join("big");
    fs::write(&file, &big).unwrap();
    let meta = DeviceMeta {
        device_id: "aaaaaaaa-0000-0000-0000-000000000001".into(),
        device_name: "Laptop".into(),
        saved_at: "2027-03-03T10:00:00.000Z".into(),
        seq: 1,
    };
    drive.upload_device(&meta, &file).unwrap();
    {
        let st = mock.state.lock().unwrap();
        assert!(
            st.chunk_ranges.len() > 20,
            "{} chunks",
            st.chunk_ranges.len()
        );
        assert!(st.chunk_ranges[0].starts_with("bytes 0-262143/"));
    }
    let out = dir.join("out");
    drive.download_device(&meta.device_id, &out).unwrap();
    assert_eq!(fs::read(&out).unwrap(), big);

    // A replacement that dies half way leaves the old content in place.
    let newer: Vec<u8> = vec![7u8; 6 * 1024 * 1024];
    fs::write(&file, &newer).unwrap();
    mock.state.lock().unwrap().chunk_ranges.clear();
    let mut next = meta.clone();
    next.seq = 2;
    // The server loses the session after the first chunk.
    let state = mock.state.clone();
    let killer = std::thread::spawn(move || loop {
        let mut st = state.lock().unwrap();
        if st.chunk_ranges.len() >= 3 {
            st.sessions.clear();
            return;
        }
        drop(st);
        std::thread::sleep(std::time::Duration::from_millis(1));
    });
    assert!(drive.upload_device(&next, &file).is_err());
    killer.join().unwrap();
    drive.download_device(&meta.device_id, &out).unwrap();
    assert_eq!(
        fs::read(&out).unwrap(),
        big,
        "the old snapshot is untouched"
    );
    assert_eq!(drive.list_devices().unwrap()[0].seq, 1);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_dropped_connection_resumes_from_where_the_server_got_to() {
    let mock = start_mock();
    let dir = temp_dir("drive-resume");
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    let data: Vec<u8> = (0..6 * 1024 * 1024).map(|i| (i % 241) as u8).collect();
    let file = dir.join("data");
    fs::write(&file, &data).unwrap();
    // The server keeps two chunks but never answers.
    mock.state.lock().unwrap().drop_after_store = 2;
    drive.upload_media(&"cd".repeat(32), &file).unwrap();
    let out = dir.join("out");
    drive.download_media(&"cd".repeat(32), &out).unwrap();
    assert_eq!(fs::read(&out).unwrap(), data);
    // No byte was sent twice: the ranges are contiguous.
    let st = mock.state.lock().unwrap();
    let mut expect = 0u64;
    for r in &st.chunk_ranges {
        let span = r.strip_prefix("bytes ").unwrap().split('/').next().unwrap();
        let (a, b) = span.split_once('-').unwrap();
        assert_eq!(a.parse::<u64>().unwrap(), expect, "{r}");
        expect = b.parse::<u64>().unwrap() + 1;
    }
    assert_eq!(expect, data.len() as u64);
    drop(st);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn temporary_failures_are_retried_and_an_expired_token_is_refreshed_once() {
    let mock = start_mock();
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    // Two 503s then success.
    mock.state.lock().unwrap().fail_next = vec![(503, "{}".into()), (503, "{}".into())];
    assert_eq!(drive.account().unwrap(), "me@example.com");
    assert_eq!(requests(&mock, "GET /drive/v3/about"), 3);

    // The access token is rejected once: a new one is fetched and the call goes through.
    let before = mock.state.lock().unwrap().token_calls;
    mock.state.lock().unwrap().expire_first_token = true;
    assert_eq!(drive.account().unwrap(), "me@example.com");
    assert_eq!(mock.state.lock().unwrap().token_calls, before + 1);
}

#[test]
fn problems_are_explained_in_words() {
    let mock = start_mock();
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    mock.state.lock().unwrap().fail_next = vec![(
        403,
        r#"{"error":{"message":"The user's Drive storage quota has been exceeded.","errors":[{"reason":"storageQuotaExceeded"}]}}"#.into(),
    )];
    let err = drive.account().unwrap_err();
    assert!(
        matches!(&err, SyncError::Drive(m) if m.contains("full")),
        "{err:?}"
    );

    mock.state.lock().unwrap().fail_next = vec![(
        403,
        r#"{"error":{"message":"Drive API has not been used","errors":[{"reason":"accessNotConfigured"}]}}"#.into(),
    )];
    assert!(
        matches!(drive.account().unwrap_err(), SyncError::Drive(m) if m.contains("isn't turned on"))
    );

    // A refresh token Google no longer accepts needs the user to sign in again.
    let revoked = remote(&mock, "revoked", 256 * 1024);
    let err = revoked.account().unwrap_err();
    assert!(
        matches!(&err, SyncError::Auth(m) if m.contains("Connect again")),
        "{err:?}"
    );
    assert!(err.needs_attention());

    // No server at all is "offline", worth retrying later.
    let gone = {
        let m = start_mock();
        let r = remote(&m, "refresh-1", 256 * 1024);
        r.account().unwrap();
        drop(m);
        r
    };
    // The cached access token is still valid, so the failure is in the API call itself.
    let err = gone.account().unwrap_err();
    assert!(matches!(err, SyncError::Offline(_)), "{err:?}");
    assert!(err.is_transient());
}

#[test]
fn listings_are_paged_and_files_that_are_not_minimaps_are_ignored() {
    let mock = start_mock();
    let dir = temp_dir("drive-paging");
    mock.state.lock().unwrap().page_cap = 2;
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    let f = dir.join("x");
    fs::write(&f, b"checkpoint").unwrap();
    let dev = "aaaaaaaa-0000-0000-0000-000000000001";
    for hour in 10..17 {
        drive
            .upload_checkpoint(&format!("{dev}-20270303-{hour}0000.db.enc"), &f)
            .unwrap();
    }
    // A stray file someone put in the checkpoints folder is not listed.
    {
        let mut st = mock.state.lock().unwrap();
        let parent = st
            .files
            .iter()
            .find(|f| f.name == "checkpoints")
            .unwrap()
            .id
            .clone();
        st.files.push(MFile {
            id: "stray".into(),
            name: "holiday.jpg".into(),
            parent: Some(parent),
            folder: false,
            content: vec![1],
            props: HashMap::new(),
            created: stamp(900),
            modified: stamp(900),
        });
    }
    assert_eq!(drive.list_checkpoints().unwrap().len(), 7);
    assert!(
        requests(&mock, "GET /drive/v3/files") >= 4,
        "several pages were read"
    );
    // Deleting one checkpoint touches that file only.
    drive
        .delete_checkpoint(&format!("{dev}-20270303-100000.db.enc"))
        .unwrap();
    assert_eq!(drive.list_checkpoints().unwrap().len(), 6);
    assert!(mock
        .state
        .lock()
        .unwrap()
        .files
        .iter()
        .any(|f| f.id == "stray"));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn two_devices_that_both_made_the_folder_agree_on_the_older_one() {
    let mock = start_mock();
    let dir = temp_dir("drive-folders");
    {
        let mut st = mock.state.lock().unwrap();
        for (n, id) in [(1u32, "old"), (2, "new")] {
            st.files.push(MFile {
                id: id.into(),
                name: "Minimap".into(),
                parent: None,
                folder: true,
                content: vec![],
                props: HashMap::new(),
                created: stamp(n),
                modified: stamp(n),
            });
        }
    }
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    drive.write_vault(b"vault bytes").unwrap();
    let st = mock.state.lock().unwrap();
    let vault = st.files.iter().find(|f| f.name == "vault.json").unwrap();
    assert_eq!(vault.parent.as_deref(), Some("old"));
    drop(st);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_remotes_clock_is_compared_with_ours() {
    let mock = start_mock();
    let drive = remote(&mock, "refresh-1", 256 * 1024);
    assert_eq!(drive.clock_skew_seconds(), None);
    mock.state.lock().unwrap().date_offset_secs = 400; // Google's clock is 400 s ahead
    drive.account().unwrap();
    let skew = drive.clock_skew_seconds().unwrap();
    assert!((-402..=-398).contains(&skew), "{skew}");
}

#[test]
fn queries_escape_quotes_and_backslashes() {
    assert_eq!(q_escape("it's"), "it\\'s");
    assert_eq!(q_escape("a\\b"), "a\\\\b");
}

#[test]
fn the_whole_sign_in_works_against_a_local_google() {
    let mock = start_mock();
    let http = http_client().unwrap();
    let ends = endpoints(&mock);
    let seen_url = Arc::new(Mutex::new(String::new()));
    let seen = seen_url.clone();
    // The "browser": reads the sign-in address, then follows the redirect Google would send.
    let open = move |url: &str| -> Result<()> {
        *seen.lock().unwrap() = url.to_owned();
        let query = url.split_once('?').unwrap().1;
        let get = |name: &str| {
            query
                .split('&')
                .find_map(|p| p.strip_prefix(&format!("{name}=")))
                .map(url_decode)
                .unwrap()
        };
        let (redirect, state) = (get("redirect_uri"), get("state"));
        std::thread::spawn(move || {
            let port = redirect
                .rsplit(':')
                .next()
                .unwrap()
                .trim_end_matches('/')
                .to_owned();
            let mut s = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
            write!(
                s,
                "GET /?code=auth-code-1&state={state} HTTP/1.1\r\nHost: x\r\n\r\n"
            )
            .unwrap();
            let mut sink = String::new();
            let _ = s.read_to_string(&mut sink);
        });
        Ok(())
    };
    let tokens = crate::oauth::sign_in(
        &http,
        &ends,
        &creds(),
        &open,
        &AtomicBool::new(false),
        std::time::Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(tokens.refresh_token, "refresh-1");
    // The exchange proved the PKCE verifier: its challenge is what the address carried.
    let url = seen_url.lock().unwrap().clone();
    let challenge = url
        .split("code_challenge=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    let form = mock.state.lock().unwrap().token_forms[0].clone();
    assert_eq!(
        crate::oauth::challenge_for(&form["code_verifier"]),
        challenge
    );
    assert_eq!(form["code"], "auth-code-1");
    assert_eq!(form["client_secret"], "client-secret");
    assert!(form["redirect_uri"].starts_with("http://127.0.0.1:"));

    // The refresh token then gives access tokens, and revoking is best effort.
    let source = TokenSource::new(http, ends, creds(), tokens.refresh_token.clone());
    assert!(source.access_token().unwrap().starts_with("access-"));
    source.revoke();
    assert_eq!(requests(&mock, "POST /revoke"), 1);
}
