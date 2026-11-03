//! ChatGPT through OpenAI's "Sign in with ChatGPT" for open-source and
//! locally hosted apps: the student signs in in the browser, Know Giver
//! registers itself under its own name, and eligible requests use the
//! student's ChatGPT plan through the Responses API.
//!
//! The flow follows <https://developers.openai.com/siwc/token-sharing-open-source>:
//! PKCE with a loopback callback on `127.0.0.1`, an ID token checked against
//! OpenAI's published keys, rotating refresh tokens refreshed one at a time,
//! and revocation on sign-out. Tokens live in memory here; the engine saves
//! the session in the app's database ([`take_session_update`],
//! [`restore_session`]).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::AiError;
use crate::account::{AccountState, LoginEvent, LoginPages};

/// Marker base URL saved in the settings; requests go to [`API_BASE`].
pub(crate) const BASE_URL: &str = "account://chatgpt";
const API_BASE: &str = "https://api.openai.com/v1";
const ISSUER: &str = "https://auth.openai.com";
const AUTHORIZE_URL: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const DISCOVERY_URL: &str = "https://auth.openai.com/.well-known/openid-configuration";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
/// The client a new installation registers through.
const REGISTRATION_CLIENT: &str = "dynamic_agent_client";
/// The name OpenAI shows the student; the same on every installation.
const AGENT_NAME: &str = "Know Giver";
const CALLBACK_PATH: &str = "/auth/callback";
/// The documented port first, then any free one (only the port may vary).
const PORTS: [u16; 3] = [1455, 1457, 0];
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);
/// Refresh this long before the access token expires.
const REFRESH_MARGIN: u64 = 120;

pub(crate) fn is_endpoint(base_url: &str) -> bool {
    base_url.trim().trim_end_matches('/') == BASE_URL
}

fn error(message: impl Into<String>) -> AiError {
    AiError::ChatGpt(message.into())
}

/// One installation's registration with OpenAI and its current tokens.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatGptSession {
    /// Stable per installation, saved before the first sign-in.
    pub host_id: String,
    /// Issued by OpenAI on the first sign-in (`oaiapp_…`).
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    /// Unix seconds.
    pub expires_at: u64,
    pub email: String,
}

impl ChatGptSession {
    fn signed_in(&self) -> bool {
        !self.client_id.is_empty() && !self.refresh_token.is_empty()
    }
}

impl std::fmt::Debug for ChatGptSession {
    /// Never prints tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatGptSession")
            .field("client_id", &self.client_id)
            .field("signed_in", &self.signed_in())
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for ChatGptSession {
    /// Never prints tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChatGPT session for {}", self.email)
    }
}

struct Store {
    session: Option<ChatGptSession>,
    /// The session changed and the engine has not saved it yet.
    dirty: bool,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    session: None,
    dirty: false,
});
/// Serializes refreshes: refresh tokens rotate, so two at once would race.
static REFRESH: Mutex<()> = Mutex::new(());

fn store() -> MutexGuard<'static, Store> {
    STORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn update(session: Option<ChatGptSession>) {
    let mut store = store();
    store.session = session;
    store.dirty = true;
}

fn current() -> Option<ChatGptSession> {
    store().session.clone()
}

/// Gives the engine a session saved earlier.
pub fn restore_session(session: ChatGptSession) {
    let mut store = store();
    if store.session.is_none() {
        store.session = Some(session);
    }
}

/// The session to save, once after each change (sign-in, refresh,
/// sign-out); `Some(None)` means "delete the saved session".
pub fn take_session_update() -> Option<Option<ChatGptSession>> {
    let mut store = store();
    if !store.dirty {
        return None;
    }
    store.dirty = false;
    Some(store.session.clone())
}

/// The signed-in account's email, for the UI.
pub fn signed_in_email() -> Option<String> {
    current()
        .filter(ChatGptSession::signed_in)
        .map(|session| session.email)
}

pub(crate) fn state() -> AccountState {
    if current().is_some_and(|s| s.signed_in()) {
        AccountState::Ready
    } else {
        AccountState::SignedOut
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn random_text(bytes: usize) -> Result<String, AiError> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|err| error(format!("no random source: {err}")))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(600)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// What one sign-in attempt sends to the authorize page.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Attempt {
    verifier: String,
    state: String,
    nonce: String,
    redirect_uri: String,
    /// The issued client when this installation registered before.
    client_id: String,
    host_id: String,
}

impl Attempt {
    fn registering(&self) -> bool {
        self.client_id.is_empty()
    }

    fn authorize_url(&self) -> String {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()));
        let client = if self.registering() {
            REGISTRATION_CLIENT
        } else {
            &self.client_id
        };
        let mut params = vec![
            ("response_type", "code"),
            ("client_id", client),
            ("redirect_uri", &self.redirect_uri),
            ("scope", SCOPE),
            ("resource", RESOURCE),
            ("state", &self.state),
            ("nonce", &self.nonce),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("ext_agent_host_id", &self.host_id),
        ];
        if self.registering() {
            params.push(("agent_name_hint", AGENT_NAME));
        }
        let query: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect();
        format!("{AUTHORIZE_URL}?{}", query.join("&"))
    }
}

/// Percent-encodes a query value.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What OpenAI sent back to the loopback page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Callback {
    code: String,
    state: String,
    client_id: String,
    error: String,
}

fn parse_callback(request_line: &str) -> Option<Callback> {
    let target = request_line.strip_prefix("GET ")?.split(' ').next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != CALLBACK_PATH {
        return None;
    }
    let mut callback = Callback::default();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = decode(value);
        match key {
            "code" => callback.code = value,
            "state" => callback.state = value,
            "client_id" => callback.client_id = value,
            "error" => callback.error = value,
            _ => {}
        }
    }
    Some(callback)
}

/// Checks the callback against the attempt and returns the code and the
/// client to exchange it with.
fn accept_callback(attempt: &Attempt, callback: &Callback) -> Result<(String, String), AiError> {
    if callback.state != attempt.state {
        return Err(error("sign-in answer does not match this attempt"));
    }
    if !callback.error.is_empty() {
        return Err(if callback.error == "access_denied" {
            AiError::SignInCancelled
        } else {
            error(format!("sign-in refused: {}", callback.error))
        });
    }
    if callback.code.is_empty() {
        return Err(error("sign-in answer has no code"));
    }
    let client_id = if attempt.registering() {
        if callback.client_id.is_empty() {
            return Err(error("OpenAI did not finish registering the app"));
        }
        callback.client_id.clone()
    } else {
        if !callback.client_id.is_empty() && callback.client_id != attempt.client_id {
            return Err(error("sign-in answer is for another registration"));
        }
        attempt.client_id.clone()
    };
    Ok((callback.code.clone(), client_id))
}

fn bind() -> Result<(TcpListener, u16), AiError> {
    for port in PORTS {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            let port = listener
                .local_addr()
                .map_err(|err| error(format!("no local port: {err}")))?
                .port();
            return Ok((listener, port));
        }
    }
    Err(error("could not open a local port for the sign-in"))
}

pub(crate) fn login(
    pages: &LoginPages,
    mut on_event: impl FnMut(LoginEvent<'_>),
) -> Result<(), AiError> {
    let mut session = current().unwrap_or_default();
    if session.host_id.is_empty() {
        session.host_id = random_text(24)?;
        // Saved before the browser opens, as OpenAI asks.
        update(Some(session.clone()));
    }
    let (listener, port) = bind()?;
    let attempt = Attempt {
        verifier: random_text(48)?,
        state: random_text(24)?,
        nonce: random_text(24)?,
        redirect_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        client_id: session.client_id.clone(),
        host_id: session.host_id.clone(),
    };
    on_event(LoginEvent::OpenUrl(&attempt.authorize_url()));
    let callback = wait_for_callback(&listener, pages)?;
    let (code, client_id) = accept_callback(&attempt, &callback)?;
    let agent = agent();
    let tokens = post_form(
        &agent,
        TOKEN_URL,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", &code),
            ("code_verifier", &attempt.verifier),
            ("redirect_uri", &attempt.redirect_uri),
            ("resource", RESOURCE),
        ],
    )?;
    let tokens = Tokens::parse(&tokens)?;
    if !tokens.scopes_include(PLAN_SCOPE) {
        return Err(AiError::PlanNotIncluded);
    }
    let claims = verify_id_token(&agent, &tokens.id_token, &client_id, Some(&attempt.nonce))?;
    session.client_id = client_id;
    session.email = claims["email"].as_str().unwrap_or_default().to_owned();
    tokens.apply(&mut session);
    update(Some(session));
    Ok(())
}

fn wait_for_callback(listener: &TcpListener, pages: &LoginPages) -> Result<Callback, AiError> {
    listener
        .set_nonblocking(true)
        .map_err(|err| error(format!("local port: {err}")))?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(callback) = answer(stream, pages) {
                    return Ok(callback);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err(AiError::SignInCancelled);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(error(format!("local port: {err}"))),
        }
    }
}

/// Reads one browser request; returns the callback when it is the one.
fn answer(mut stream: TcpStream, pages: &LoginPages) -> Option<Callback> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    BufReader::new(stream.try_clone().ok()?)
        .take(16 * 1024)
        .read_line(&mut line)
        .ok()?;
    let callback = parse_callback(line.trim_end());
    let (status, text) = match &callback {
        Some(c) if c.error.is_empty() && !c.code.is_empty() => ("200 OK", pages.success.as_str()),
        Some(_) => ("200 OK", pages.failure.as_str()),
        None => ("404 Not Found", ""),
    };
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Know Giver</title>\
         <body style=\"font-family:sans-serif;max-width:32rem;margin:4rem auto\"><p>{}</p>",
        html_escape(text)
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    callback
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn post_form(agent: &ureq::Agent, url: &str, form: &[(&str, &str)]) -> Result<Value, AiError> {
    let body: Vec<String> = form
        .iter()
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect();
    let mut response = agent
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body.join("&"))
        .map_err(|err| AiError::Connection {
            url: url.to_owned(),
            reason: err.to_string(),
        })?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    let json: Value = serde_json::from_str(&text).unwrap_or_default();
    match status {
        200..=299 => Ok(json),
        400 | 401 if json["error"] == "invalid_grant" => Err(AiError::Auth { status }),
        _ => Err(AiError::Http {
            status,
            body: json["error"]
                .as_str()
                .unwrap_or("token request rejected")
                .to_owned(),
        }),
    }
}

/// A token endpoint answer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    id_token: String,
    expires_in: u64,
    scope: String,
}

impl Tokens {
    fn parse(json: &Value) -> Result<Self, AiError> {
        let text = |key: &str| json[key].as_str().unwrap_or_default().to_owned();
        let tokens = Self {
            access_token: text("access_token"),
            refresh_token: text("refresh_token"),
            id_token: text("id_token"),
            expires_in: json["expires_in"].as_u64().unwrap_or(3600),
            scope: text("scope"),
        };
        if tokens.access_token.is_empty() {
            return Err(error("OpenAI sent no access token"));
        }
        Ok(tokens)
    }

    fn scopes_include(&self, scope: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == scope)
    }

    /// Replaces the session's tokens; a refresh may omit the ID token or
    /// keep the refresh token.
    fn apply(self, session: &mut ChatGptSession) {
        session.access_token = self.access_token;
        if !self.refresh_token.is_empty() {
            session.refresh_token = self.refresh_token;
        }
        if !self.id_token.is_empty() {
            session.id_token = self.id_token;
        }
        session.expires_at = now() + self.expires_in;
    }
}

/// Checks the ID token's signature against OpenAI's published keys and its
/// issuer, audience, expiry and nonce. Returns its claims.
fn verify_id_token(
    agent: &ureq::Agent,
    token: &str,
    client_id: &str,
    nonce: Option<&str>,
) -> Result<Value, AiError> {
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(error("ID token is malformed"));
    };
    let decode_part = |part: &str| -> Result<Vec<u8>, AiError> {
        URL_SAFE_NO_PAD
            .decode(part)
            .map_err(|_| error("ID token is malformed"))
    };
    let header: Value = serde_json::from_slice(&decode_part(header)?)
        .map_err(|_| error("ID token is malformed"))?;
    let claims: Value = serde_json::from_slice(&decode_part(payload)?)
        .map_err(|_| error("ID token is malformed"))?;
    if header["alg"] != "RS256" {
        return Err(error("ID token uses an unexpected algorithm"));
    }
    let kid = header["kid"].as_str().unwrap_or_default();
    let keys = fetch_keys(agent)?;
    let key = keys["keys"]
        .as_array()
        .and_then(|keys| keys.iter().find(|k| k["kid"] == kid))
        .ok_or_else(|| error("ID token key is not published"))?;
    let n = decode_part(key["n"].as_str().unwrap_or_default())?;
    let e = decode_part(key["e"].as_str().unwrap_or_default())?;
    let signed = &token[..token.rfind('.').unwrap_or(0)];
    ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            signed.as_bytes(),
            &decode_part(signature)?,
        )
        .map_err(|_| error("ID token signature is invalid"))?;
    check_claims(&claims, client_id, nonce, now())?;
    Ok(claims)
}

fn check_claims(
    claims: &Value,
    client_id: &str,
    nonce: Option<&str>,
    now: u64,
) -> Result<(), AiError> {
    if claims["iss"].as_str().map(|i| i.trim_end_matches('/')) != Some(ISSUER) {
        return Err(error("ID token comes from another issuer"));
    }
    let audience_ok = match &claims["aud"] {
        Value::String(aud) => aud == client_id,
        Value::Array(list) => list.iter().any(|a| a == client_id),
        _ => false,
    };
    if !audience_ok {
        return Err(error("ID token is for another app"));
    }
    if claims["exp"].as_u64().is_none_or(|exp| exp + 300 < now) {
        return Err(error("ID token has expired"));
    }
    if let Some(nonce) = nonce
        && claims["nonce"].as_str() != Some(nonce)
    {
        return Err(error("ID token does not match this sign-in"));
    }
    Ok(())
}

fn fetch_keys(agent: &ureq::Agent) -> Result<Value, AiError> {
    let discovery = get_json(agent, DISCOVERY_URL)?;
    let jwks = discovery["jwks_uri"]
        .as_str()
        .ok_or_else(|| error("OpenAI publishes no signing keys"))?;
    get_json(agent, jwks)
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<Value, AiError> {
    let mut response = agent.get(url).call().map_err(|err| AiError::Connection {
        url: url.to_owned(),
        reason: err.to_string(),
    })?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(AiError::Http {
            status,
            body: format!("could not read {url}"),
        });
    }
    response
        .body_mut()
        .read_json()
        .map_err(|err| AiError::InvalidResponse(err.to_string()))
}

/// A valid access token, refreshed first when it is about to expire.
fn access_token(force_refresh: bool) -> Result<String, AiError> {
    let session = current()
        .filter(ChatGptSession::signed_in)
        .ok_or(AiError::Auth { status: 401 })?;
    if !force_refresh && session.expires_at > now() + REFRESH_MARGIN {
        return Ok(session.access_token);
    }
    let _one_at_a_time = REFRESH
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Another request may have refreshed while this one waited.
    let mut session = current()
        .filter(ChatGptSession::signed_in)
        .ok_or(AiError::Auth { status: 401 })?;
    if !force_refresh && session.expires_at > now() + REFRESH_MARGIN {
        return Ok(session.access_token);
    }
    let json = post_form(
        &agent(),
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &session.client_id),
            ("refresh_token", &session.refresh_token),
            ("resource", RESOURCE),
        ],
    );
    let json = match json {
        Ok(json) => json,
        Err(AiError::Auth { status }) => {
            // The refresh token is gone: the student must sign in again.
            session.access_token.clear();
            session.refresh_token.clear();
            update(Some(session));
            return Err(AiError::Auth { status });
        }
        Err(err) => return Err(err),
    };
    Tokens::parse(&json)?.apply(&mut session);
    let token = session.access_token.clone();
    update(Some(session));
    Ok(token)
}

/// Revokes the refresh token (best effort) and forgets the tokens. The
/// registration and host stay, so signing in again reuses them.
pub(crate) fn sign_out() {
    let Some(mut session) = current() else {
        return;
    };
    if !session.refresh_token.is_empty() {
        let agent = agent();
        let revoked = get_json(&agent, DISCOVERY_URL).and_then(|discovery| {
            let url = discovery["revocation_endpoint"]
                .as_str()
                .ok_or_else(|| error("no revocation endpoint"))?
                .to_owned();
            post_form(
                &agent,
                &url,
                &[
                    ("token", &session.refresh_token),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", &session.client_id),
                ],
            )
            .map(|_| ())
            .or_else(|err| match err {
                // An empty 200 is success; an already-invalid token too.
                AiError::Http { status: 200, .. } => Ok(()),
                other => Err(other),
            })
        });
        if let Err(err) = revoked {
            tracing::warn!(%err, "could not confirm the ChatGPT sign-out with OpenAI");
        }
    }
    session.access_token.clear();
    session.refresh_token.clear();
    session.id_token.clear();
    session.expires_at = 0;
    session.email.clear();
    update(Some(session));
}

pub(crate) fn list_models() -> Result<Vec<String>, AiError> {
    let token = access_token(false)?;
    let url = format!("{API_BASE}/models");
    let mut response = agent()
        .get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|err| AiError::Connection {
            url: url.clone(),
            reason: err.to_string(),
        })?;
    let status = response.status().as_u16();
    match status {
        200..=299 => {}
        401 | 403 => return Err(AiError::Auth { status }),
        _ => {
            return Err(AiError::Http {
                status,
                body: "model list rejected".into(),
            });
        }
    }
    let json: Value = response
        .body_mut()
        .read_json()
        .map_err(|err| AiError::InvalidResponse(err.to_string()))?;
    let models = parse_models(&json);
    if models.is_empty() {
        return Err(AiError::InvalidResponse("empty model list".into()));
    }
    Ok(models)
}

/// Display-ready models in the server's order (`visibility: "list"`).
fn parse_models(json: &Value) -> Vec<String> {
    json["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter(|m| m["visibility"].as_str().is_none_or(|v| v == "list"))
                .filter_map(|m| m["slug"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The Responses request for chat messages in the OpenAI shape: system
/// messages become `instructions`, `image_url` parts become `input_image`.
/// Sampling and history parameters are left out, as the plan route asks.
fn responses_body(model: &str, messages: &Value) -> Result<Value, AiError> {
    let list = messages
        .as_array()
        .ok_or_else(|| AiError::InvalidResponse("missing chat messages".into()))?;
    let mut instructions = String::new();
    let mut input = Vec::new();
    for message in list {
        let role = message["role"].as_str().unwrap_or("user");
        if role == "system" {
            if !instructions.is_empty() {
                instructions.push_str("\n\n");
            }
            instructions.push_str(message["content"].as_str().unwrap_or_default());
            continue;
        }
        let content = match &message["content"] {
            Value::Array(parts) => Value::Array(
                parts
                    .iter()
                    .filter_map(|part| match part["type"].as_str() {
                        Some("text") => Some(json!({
                            "type": if role == "assistant" { "output_text" } else { "input_text" },
                            "text": part["text"]
                        })),
                        Some("image_url") => Some(json!({
                            "type": "input_image",
                            "image_url": part["image_url"]["url"],
                            "detail": part["image_url"]["detail"].as_str().unwrap_or("auto")
                        })),
                        _ => None,
                    })
                    .collect(),
            ),
            other => other.clone(),
        };
        input.push(json!({"role": role, "content": content}));
    }
    if input.is_empty() {
        return Err(AiError::InvalidResponse(
            "request has no user message".into(),
        ));
    }
    let mut body = json!({
        "model": model,
        "input": input,
        "store": false,
        "stream": true,
    });
    if !instructions.is_empty() {
        body["instructions"] = json!(instructions);
    }
    Ok(body)
}

pub(crate) fn chat(
    model: &str,
    messages: &Value,
    mut on_delta: impl FnMut(&str),
) -> Result<String, AiError> {
    let body = responses_body(model, messages)?;
    let mut emitted = false;
    let first = send(&body, access_token(false)?, &mut |d: &str| {
        emitted = true;
        on_delta(d);
    });
    match first {
        // An access token revoked early: refresh once, unless text already went out.
        Err(AiError::Auth { .. }) if !emitted => send(&body, access_token(true)?, &mut on_delta),
        other => other,
    }
}

fn send(body: &Value, token: String, on_delta: &mut impl FnMut(&str)) -> Result<String, AiError> {
    let url = format!("{API_BASE}/responses");
    let mut response = agent()
        .post(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "text/event-stream")
        .send_json(body)
        .map_err(|err| AiError::Connection {
            url: url.clone(),
            reason: err.to_string(),
        })?;
    let status = response.status().as_u16();
    match status {
        200..=299 => {}
        401 | 403 => return Err(AiError::Auth { status }),
        429 => return Err(AiError::PlanLimit),
        _ => {
            let text = response.body_mut().read_to_string().unwrap_or_default();
            tracing::warn!(status, body = %text.chars().take(300).collect::<String>(), "ChatGPT request rejected");
            return Err(AiError::Http {
                status,
                body: "request rejected".into(),
            });
        }
    }
    read_events(
        BufReader::new(response.body_mut().as_reader().take(4 * 1024 * 1024)),
        on_delta,
    )
}

/// Reads the Responses event stream: text deltas, then `response.completed`.
/// Anything else that ends the stream is a failure.
fn read_events(reader: impl BufRead, on_delta: &mut impl FnMut(&str)) -> Result<String, AiError> {
    let mut text = String::new();
    for line in reader.lines() {
        let line = line.map_err(|err| error(format!("stream interrupted: {err}")))?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<Value>(data.trim()) else {
            continue;
        };
        match event["type"].as_str().unwrap_or_default() {
            "response.output_text.delta" => {
                if let Some(delta) = event["delta"].as_str()
                    && !delta.is_empty()
                {
                    text.push_str(delta);
                    on_delta(delta);
                }
            }
            "response.completed" => {
                let answer = text.trim().to_owned();
                if answer.is_empty() {
                    return Err(AiError::InvalidResponse(
                        "ChatGPT returned an empty answer".into(),
                    ));
                }
                return Ok(answer);
            }
            "response.failed" | "error" => {
                let code = event["response"]["error"]["code"]
                    .as_str()
                    .or_else(|| event["error"]["code"].as_str())
                    .or_else(|| event["code"].as_str())
                    .unwrap_or_default();
                return Err(failure(code));
            }
            "response.incomplete" => {
                return Err(AiError::OutOfTokens);
            }
            _ => {}
        }
    }
    Err(error("stream ended before the answer was complete"))
}

fn failure(code: &str) -> AiError {
    match code {
        "subscription_sharing_usage_limit_exceeded"
        | "subscription_sharing_usage_unavailable"
        | "rate_limit_exceeded"
        | "insufficient_quota" => AiError::PlanLimit,
        "" => error("ChatGPT could not answer"),
        other => error(format!("ChatGPT could not answer: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt(client_id: &str) -> Attempt {
        Attempt {
            verifier: "verifier".into(),
            state: "state-1".into(),
            nonce: "nonce-1".into(),
            redirect_uri: "http://127.0.0.1:1455/auth/callback".into(),
            client_id: client_id.into(),
            host_id: "host-1".into(),
        }
    }

    fn param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
        url.split_once('?')?
            .1
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
    }

    #[test]
    fn a_first_sign_in_registers_the_app_under_its_name() {
        let url = attempt("").authorize_url();
        assert!(url.starts_with(AUTHORIZE_URL));
        assert_eq!(param(&url, "client_id"), Some("dynamic_agent_client"));
        assert_eq!(param(&url, "agent_name_hint"), Some("Know%20Giver"));
        assert_eq!(param(&url, "ext_agent_host_id"), Some("host-1"));
        assert_eq!(param(&url, "code_challenge_method"), Some("S256"));
        assert_eq!(
            param(&url, "redirect_uri"),
            Some("http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback")
        );
        let scope = decode(param(&url, "scope").unwrap());
        assert!(scope.split(' ').any(|s| s == "chatgpt.tokens.use.direct"));
        assert!(scope.split(' ').any(|s| s == "offline_access"));
        // The challenge is the SHA-256 of the verifier, not the verifier.
        assert_eq!(
            param(&url, "code_challenge"),
            Some(URL_SAFE_NO_PAD.encode(Sha256::digest(b"verifier")).as_str())
        );
    }

    #[test]
    fn a_later_sign_in_reuses_the_issued_client() {
        let url = attempt("oaiapp_123").authorize_url();
        assert_eq!(param(&url, "client_id"), Some("oaiapp_123"));
        assert_eq!(param(&url, "agent_name_hint"), None);
    }

    #[test]
    fn the_callback_is_read_from_the_request_line() {
        let callback =
            parse_callback("GET /auth/callback?code=abc&state=state-1&client_id=oaiapp_9 HTTP/1.1")
                .unwrap();
        assert_eq!(callback.code, "abc");
        assert_eq!(callback.client_id, "oaiapp_9");
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1"), None);
    }

    #[test]
    fn registration_needs_the_issued_client_and_a_matching_state() {
        let fresh = attempt("");
        let ok = Callback {
            code: "abc".into(),
            state: "state-1".into(),
            client_id: "oaiapp_9".into(),
            error: String::new(),
        };
        assert_eq!(
            accept_callback(&fresh, &ok).unwrap(),
            ("abc".into(), "oaiapp_9".into())
        );
        let no_client = Callback {
            client_id: String::new(),
            ..ok.clone()
        };
        assert!(accept_callback(&fresh, &no_client).is_err());
        let wrong_state = Callback {
            state: "other".into(),
            ..ok.clone()
        };
        assert!(accept_callback(&fresh, &wrong_state).is_err());
        let denied = Callback {
            error: "access_denied".into(),
            code: String::new(),
            ..ok.clone()
        };
        assert!(matches!(
            accept_callback(&fresh, &denied),
            Err(AiError::SignInCancelled)
        ));
        // A returning sign-in rejects an answer for another registration.
        let returning = attempt("oaiapp_1");
        assert!(accept_callback(&returning, &ok).is_err());
        let omitted = Callback {
            client_id: String::new(),
            ..ok
        };
        assert_eq!(accept_callback(&returning, &omitted).unwrap().1, "oaiapp_1");
    }

    #[test]
    fn id_token_claims_must_match_the_app_and_the_attempt() {
        let claims = json!({
            "iss": "https://auth.openai.com",
            "aud": ["oaiapp_9"],
            "exp": 2_000,
            "nonce": "nonce-1",
            "email": "student@example.org"
        });
        assert!(check_claims(&claims, "oaiapp_9", Some("nonce-1"), 1_000).is_ok());
        assert!(check_claims(&claims, "oaiapp_other", Some("nonce-1"), 1_000).is_err());
        assert!(check_claims(&claims, "oaiapp_9", Some("nonce-2"), 1_000).is_err());
        assert!(check_claims(&claims, "oaiapp_9", Some("nonce-1"), 3_000).is_err());
        let other_issuer = json!({"iss": "https://example.org", "aud": "oaiapp_9", "exp": 2_000});
        assert!(check_claims(&other_issuer, "oaiapp_9", None, 1_000).is_err());
    }

    #[test]
    fn plan_usage_needs_its_scope() {
        let tokens = Tokens::parse(&json!({
            "access_token": "a", "refresh_token": "r", "id_token": "i",
            "expires_in": 3600, "scope": "openid email offline_access"
        }))
        .unwrap();
        assert!(!tokens.scopes_include(PLAN_SCOPE));
        let tokens = Tokens::parse(&json!({
            "access_token": "a", "scope": "openid chatgpt.tokens.use.direct"
        }))
        .unwrap();
        assert!(tokens.scopes_include(PLAN_SCOPE));
        assert!(Tokens::parse(&json!({"scope": "openid"})).is_err());
    }

    #[test]
    fn a_refresh_keeps_what_it_does_not_replace() {
        let mut session = ChatGptSession {
            refresh_token: "old-refresh".into(),
            id_token: "old-id".into(),
            ..ChatGptSession::default()
        };
        Tokens::parse(&json!({"access_token": "new", "expires_in": 60}))
            .unwrap()
            .apply(&mut session);
        assert_eq!(session.access_token, "new");
        assert_eq!(session.refresh_token, "old-refresh");
        assert_eq!(session.id_token, "old-id");
        Tokens::parse(&json!({"access_token": "newer", "refresh_token": "rotated"}))
            .unwrap()
            .apply(&mut session);
        assert_eq!(session.refresh_token, "rotated");
    }

    #[test]
    fn requests_follow_the_plan_route_rules() {
        let messages = json!([
            {"role": "system", "content": "Teach."},
            {"role": "user", "content": "Explain."},
            {"role": "assistant", "content": "Here."},
            {"role": "user", "content": [
                {"type": "text", "text": "Read this."},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA", "detail": "high"}}
            ]}
        ]);
        let body = responses_body("gpt-x", &messages).unwrap();
        assert_eq!(body["instructions"], "Teach.");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body.get("temperature").is_none());
        let input = body["input"].as_array().unwrap();
        assert_eq!(input.len(), 3);
        assert!(input.iter().all(|m| m["role"] != "system"));
        assert_eq!(input[2]["content"][0]["type"], "input_text");
        assert_eq!(input[2]["content"][1]["type"], "input_image");
        assert_eq!(
            input[2]["content"][1]["image_url"],
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn the_stream_counts_only_a_completed_answer() {
        let stream = "event: response.output_text.delta\n\
            data: {\"type\":\"response.output_text.delta\",\"delta\":\"Bra\"}\n\n\
            data: {\"type\":\"response.output_text.delta\",\"delta\":\"sília\"}\n\n\
            data: {\"type\":\"response.completed\",\"response\":{}}\n\n";
        let mut seen = String::new();
        let answer = read_events(stream.as_bytes(), &mut |d: &str| seen.push_str(d)).unwrap();
        assert_eq!(answer, "Brasília");
        assert_eq!(seen, "Brasília");
        let cut = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Bra\"}\n\n";
        assert!(read_events(cut.as_bytes(), &mut |_: &str| {}).is_err());
    }

    #[test]
    fn a_used_up_plan_is_reported_as_such() {
        let stream = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"subscription_sharing_usage_limit_exceeded\"}}}\n\n";
        assert!(matches!(
            read_events(stream.as_bytes(), &mut |_: &str| {}),
            Err(AiError::PlanLimit)
        ));
    }

    #[test]
    fn only_listed_models_are_offered_in_order() {
        let json = json!({"models": [
            {"slug": "gpt-b", "visibility": "list"},
            {"slug": "gpt-hidden", "visibility": "hide"},
            {"slug": "gpt-a", "visibility": "list"}
        ]});
        assert_eq!(parse_models(&json), vec!["gpt-b", "gpt-a"]);
    }

    #[test]
    fn query_values_round_trip() {
        let value = "http://127.0.0.1:1455/auth/callback?x=1&y=á";
        assert_eq!(decode(&encode(value)), value);
    }

    #[test]
    fn a_stray_percent_sign_is_kept() {
        assert_eq!(decode("50%"), "50%");
        assert_eq!(decode("a%zzb"), "a%zzb");
        assert_eq!(decode("%C3%A1"), "á");
    }

    #[test]
    fn the_session_never_prints_tokens() {
        let session = ChatGptSession {
            access_token: "secret-access".into(),
            refresh_token: "secret-refresh".into(),
            email: "student@example.org".into(),
            ..ChatGptSession::default()
        };
        assert!(!session.to_string().contains("secret"));
        assert!(!format!("{session:?}").contains("secret"));
    }
}
