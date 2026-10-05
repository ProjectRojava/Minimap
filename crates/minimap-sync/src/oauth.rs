//! Signing in to Google for a desktop app (spec 22): the system browser does the sign-in, the
//! answer comes back to a one-shot server on `127.0.0.1` (loopback redirect) and the code is
//! exchanged with PKCE. Scope is `drive.file` only, so Minimap sees only files it created.
//!
//! Every endpoint is a field of [`Endpoints`] so tests can point the flow at a local server.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{Result, SyncError};

pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub revoke: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
        }
    }
}

/// An OAuth client of type "Desktop app". Google requires the secret for desktop clients and
/// treats it as non-confidential.
#[derive(Clone)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
}

impl std::fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientCredentials(<redacted>)")
    }
}

/// What a sign-in produced.
pub struct Tokens {
    pub refresh_token: String,
    pub access_token: String,
    pub expires_in: Duration,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tokens(<redacted>)")
    }
}

fn random_url_safe(bytes: usize) -> Result<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf)
        .map_err(|e| SyncError::Io(format!("no secure random source: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

/// A PKCE verifier and its S256 challenge (RFC 7636).
pub fn pkce_pair() -> Result<(String, String)> {
    let verifier = random_url_safe(48)?;
    let challenge = challenge_for(&verifier);
    Ok((verifier, challenge))
}

pub fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The address the user's browser is sent to.
pub fn authorization_url(
    endpoints: &Endpoints,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}\
         &code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent",
        endpoints.auth,
        encode(client_id),
        encode(redirect_uri),
        encode(SCOPE),
        encode(state),
        encode(challenge),
    )
}

const DONE_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Minimap</title></head>\
<body style=\"font-family:sans-serif;max-width:32em;margin:4em auto;padding:0 1em\">\
<h2>Minimap is connected to Google Drive</h2><p>You can close this tab and go back to Minimap.</p></body></html>";

const DENIED_PAGE: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Minimap</title></head>\
<body style=\"font-family:sans-serif;max-width:32em;margin:4em auto;padding:0 1em\">\
<h2>Minimap was not connected</h2><p>Google did not give Minimap access. You can close this tab.</p></body></html>";

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Connection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes());
    let _ = stream.flush();
}

/// What the browser's request to the loopback server carried.
enum Redirect {
    Code(String),
    Denied,
    /// Not the redirect (a favicon request, a stray probe).
    Other,
}

fn read_redirect(stream: &mut TcpStream, expected_state: &str) -> Redirect {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = [0u8; 4096];
    let mut got = 0;
    while got < buf.len() {
        match stream.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => {
                got += n;
                if buf[..got].windows(2).any(|w| w == b"\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf[..got]);
    let Some(line) = text.lines().next() else {
        return Redirect::Other;
    };
    let mut parts = line.split_whitespace();
    if parts.next() != Some("GET") {
        return Redirect::Other;
    }
    let Some(target) = parts.next() else {
        return Redirect::Other;
    };
    let Some((_, query)) = target.split_once('?') else {
        return Redirect::Other;
    };
    let mut code = None;
    let mut state = None;
    let mut error = false;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "code" => code = Some(decode(v)),
            "state" => state = Some(decode(v)),
            "error" => error = true,
            _ => {}
        }
    }
    // A redirect with the wrong state didn't come from the sign-in we started.
    if state.as_deref() != Some(expected_state) {
        return Redirect::Other;
    }
    if error {
        return Redirect::Denied;
    }
    code.map_or(Redirect::Other, Redirect::Code)
}

/// Waits (until `timeout`, or until `cancel` is set) for the browser to come back with a code.
fn wait_for_code(
    listener: &TcpListener,
    state: &str,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<String> {
    listener
        .set_nonblocking(true)
        .map_err(|e| SyncError::io("Couldn't wait for the sign-in", e))?;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(SyncError::Cancelled);
        }
        if started.elapsed() > timeout {
            return Err(SyncError::Auth(
                "Signing in took too long; try again".into(),
            ));
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                match read_redirect(&mut stream, state) {
                    Redirect::Code(code) => {
                        respond(&mut stream, "200 OK", DONE_PAGE);
                        return Ok(code);
                    }
                    Redirect::Denied => {
                        respond(&mut stream, "200 OK", DENIED_PAGE);
                        return Err(SyncError::Auth("Google did not give Minimap access".into()));
                    }
                    Redirect::Other => respond(&mut stream, "404 Not Found", ""),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(SyncError::io("Couldn't wait for the sign-in", e)),
        }
    }
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

fn token_error(reply: &TokenReply) -> SyncError {
    let what = reply
        .error_description
        .clone()
        .or_else(|| reply.error.clone())
        .unwrap_or_else(|| "no reason given".into());
    SyncError::Auth(what)
}

fn post_token(http: &Client, url: &str, form: &[(&str, &str)]) -> Result<TokenReply> {
    let response = http
        .post(url)
        .form(form)
        .send()
        .map_err(|e| crate::drive::network_error(&e))?;
    let status = response.status();
    let reply: TokenReply = response.json().map_err(|_| {
        SyncError::Auth(format!(
            "Google answered with an unexpected reply ({status})"
        ))
    })?;
    if !status.is_success() || reply.error.is_some() {
        return Err(token_error(&reply));
    }
    Ok(reply)
}

/// Runs the whole sign-in: opens the browser (through `open`), waits for the redirect, and
/// exchanges the code. `cancel` lets the user give up.
pub fn sign_in(
    http: &Client,
    endpoints: &Endpoints,
    creds: &ClientCredentials,
    open: &dyn Fn(&str) -> Result<()>,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Tokens> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| SyncError::io("Couldn't start the sign-in listener", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| SyncError::io("Couldn't start the sign-in listener", e))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}/");
    let (verifier, challenge) = pkce_pair()?;
    let state = random_url_safe(24)?;
    let url = authorization_url(endpoints, &creds.client_id, &redirect, &state, &challenge);
    open(&url)?;
    let code = wait_for_code(&listener, &state, cancel, timeout)?;
    let reply = post_token(
        http,
        &endpoints.token,
        &[
            ("client_id", &creds.client_id),
            ("client_secret", &creds.client_secret),
            ("code", &code),
            ("code_verifier", &verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", &redirect),
        ],
    )?;
    let refresh_token = reply.refresh_token.ok_or_else(|| {
        SyncError::Auth(
            "Google didn't give a long-lived sign-in; remove Minimap in your Google account's \
             connected apps and connect again"
                .into(),
        )
    })?;
    Ok(Tokens {
        refresh_token,
        access_token: reply.access_token.unwrap_or_default(),
        expires_in: Duration::from_secs(reply.expires_in.unwrap_or(0)),
    })
}

/// Hands out access tokens, getting a new one with the refresh token when the old one is about
/// to expire.
pub struct TokenSource {
    http: Client,
    endpoints: Endpoints,
    creds: ClientCredentials,
    refresh_token: String,
    access: Mutex<Option<(String, Instant)>>,
}

impl TokenSource {
    pub fn new(
        http: Client,
        endpoints: Endpoints,
        creds: ClientCredentials,
        refresh_token: String,
    ) -> Self {
        Self {
            http,
            endpoints,
            creds,
            refresh_token,
            access: Mutex::new(None),
        }
    }

    /// Starts with an access token already in hand (right after sign-in).
    pub fn with_access(self, token: String, valid_for: Duration) -> Self {
        *self.access.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((token, Instant::now() + valid_for));
        self
    }

    /// A token that is good for at least another minute.
    pub fn access_token(&self) -> Result<String> {
        {
            let guard = self.access.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((token, until)) = guard.as_ref() {
                if *until > Instant::now() + Duration::from_secs(60) {
                    return Ok(token.clone());
                }
            }
        }
        self.refresh()
    }

    /// Forgets the cached token and gets a new one (after Drive said 401).
    pub fn refresh(&self) -> Result<String> {
        let reply = post_token(
            &self.http,
            &self.endpoints.token,
            &[
                ("client_id", &self.creds.client_id),
                ("client_secret", &self.creds.client_secret),
                ("refresh_token", &self.refresh_token),
                ("grant_type", "refresh_token"),
            ],
        )
        .map_err(|e| match e {
            SyncError::Auth(why) => SyncError::Auth(format!(
                "Google no longer accepts the sign-in ({why}). Connect again from Settings"
            )),
            other => other,
        })?;
        let token = reply
            .access_token
            .ok_or_else(|| SyncError::Auth("Google gave no access token".into()))?;
        let valid = Duration::from_secs(reply.expires_in.unwrap_or(300));
        *self.access.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((token.clone(), Instant::now() + valid));
        Ok(token)
    }

    /// Tells Google to forget the sign-in (best effort; local forgetting is the caller's job).
    pub fn revoke(&self) {
        let _ = self
            .http
            .post(&self.endpoints.revoke)
            .form(&[("token", self.refresh_token.as_str())])
            .send();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pkce_challenge_matches_the_rfc_example() {
        // RFC 7636 appendix B.
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let (verifier, challenge) = pkce_pair().unwrap();
        assert!((43..=128).contains(&verifier.len()));
        assert_eq!(challenge_for(&verifier), challenge);
        assert_ne!(pkce_pair().unwrap().0, verifier);
    }

    #[test]
    fn the_address_asks_for_drive_file_only_and_carries_the_challenge() {
        let url = authorization_url(
            &Endpoints::default(),
            "id with space",
            "http://127.0.0.1:5000/",
            "st&te",
            "chal",
        );
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.file&"));
        assert!(url.contains("client_id=id%20with%20space"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000%2F"));
        assert!(url.contains("state=st%26te"));
        assert!(url.contains("code_challenge=chal&code_challenge_method=S256"));
        assert!(
            !url.contains("drive&") && !url.contains("auth/drive "),
            "only drive.file"
        );
    }

    #[test]
    fn percent_decoding_handles_codes_google_sends() {
        assert_eq!(decode("4%2F0AbC-d_e"), "4/0AbC-d_e");
        assert_eq!(decode("a+b"), "a b");
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz"), "%zz");
    }

    fn browser_visits(port: u16, path_and_query: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "GET {path_and_query} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
        )
        .unwrap();
        let mut reply = String::new();
        let _ = stream.read_to_string(&mut reply);
        reply
    }

    #[test]
    fn only_the_redirect_with_our_state_hands_over_a_code() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = AtomicBool::new(false);
        std::thread::scope(|s| {
            let waiter = s.spawn(|| {
                wait_for_code(&listener, "right-state", &cancel, Duration::from_secs(10))
            });
            // A favicon request, and a redirect with someone else's state, are ignored.
            assert!(browser_visits(port, "/favicon.ico").starts_with("HTTP/1.1 404"));
            assert!(browser_visits(port, "/?code=evil&state=wrong").starts_with("HTTP/1.1 404"));
            let ok = browser_visits(port, "/?code=4%2Fgood&state=right-state");
            assert!(ok.starts_with("HTTP/1.1 200") && ok.contains("Minimap is connected"));
            assert_eq!(waiter.join().unwrap().unwrap(), "4/good");
        });
    }

    #[test]
    fn refusing_access_or_cancelling_or_waiting_too_long_ends_the_wait() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let cancel = AtomicBool::new(false);
        std::thread::scope(|s| {
            let waiter =
                s.spawn(|| wait_for_code(&listener, "st", &cancel, Duration::from_secs(10)));
            let reply = browser_visits(port, "/?error=access_denied&state=st");
            assert!(reply.contains("was not connected"));
            assert!(matches!(waiter.join().unwrap(), Err(SyncError::Auth(_))));
        });
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            wait_for_code(&listener, "st", &cancelled, Duration::from_secs(10)),
            Err(SyncError::Cancelled)
        ));
        assert!(matches!(
            wait_for_code(
                &listener,
                "st",
                &AtomicBool::new(false),
                Duration::from_millis(120)
            ),
            Err(SyncError::Auth(_))
        ));
    }

    #[test]
    fn secrets_never_print() {
        let creds = ClientCredentials {
            client_id: "id".into(),
            client_secret: "super-secret".into(),
        };
        assert!(!format!("{creds:?}").contains("super-secret"));
        let tokens = Tokens {
            refresh_token: "refresh-me".into(),
            access_token: "access-me".into(),
            expires_in: Duration::from_secs(1),
        };
        assert!(!format!("{tokens:?}").contains("refresh-me"));
    }
}
