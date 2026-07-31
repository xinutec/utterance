//! Nextcloud sign-in for the browser — inert unless configured.
//!
//! **Why this exists.** The app holds recordings of two people's voices and
//! accepts uploads and deletions over open routes. On a Mac serving a LAN that
//! is fine: the network is the gate. On a public hostname it is not, and no
//! amount of obscurity substitutes.
//!
//! **Why it is inert by default.** Importing this changes nothing. The gate only
//! goes up when all three of [`SESSION_SECRET_ENV`], [`CLIENT_ID_ENV`] and
//! [`CLIENT_SECRET_ENV`] are set, so the Mac, `ng serve` and every test keep
//! running open, and only the deployed pod — where the secret lives — raises the
//! wall. A half-set configuration is treated as *off* and logged, rather than as
//! a gate with a hole in it: a wall that can be bypassed is worse than no wall,
//! because it is believed.
//!
//! **Identity only.** The OAuth access token is used once to ask Nextcloud who
//! signed in, then dropped. There is no user store here; a signed, stateless
//! cookie carries the identity, and an allowlist decides who may enter even
//! after a valid Nextcloud sign-in.
//!
//! This is the one part of the program that reads a clock. Everything in the
//! music path is deterministic on purpose (`docs/architecture.md`); an expiring
//! session cannot be, so the time is passed in rather than fetched, which keeps
//! it out of the pure code and lets the tests name their own hour.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{Query, Request};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::error::ErrorCode;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const SESSION_SECRET_ENV: &str = "UTTERANCE_SESSION_SECRET";
pub const CLIENT_ID_ENV: &str = "NC_CLIENT_ID";
pub const CLIENT_SECRET_ENV: &str = "NC_CLIENT_SECRET";
const NC_BASE_URL_ENV: &str = "NC_BASE_URL";
const NC_INTERNAL_URL_ENV: &str = "NC_INTERNAL_URL";
const REDIRECT_URI_ENV: &str = "NC_REDIRECT_URI";
const ALLOWED_USERS_ENV: &str = "UTTERANCE_ALLOWED_USERS";

const DEFAULT_NC_BASE_URL: &str = "https://dash.xinutec.org";
const DEFAULT_REDIRECT_URI: &str = "https://utterance.xinutec.org/auth/callback";

const COOKIE_NAME: &str = "utterance_session";

/// How long a sign-in lasts. Long, because the alternative is two people being
/// asked to sign in again in the middle of listening to something.
const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How long the round trip to Nextcloud and back may take.
const STATE_TTL: Duration = Duration::from_secs(10 * 60);

type HmacSha256 = Hmac<Sha256>;

/// Everything the gate needs. Its absence is what "no gate" means.
#[derive(Clone, Debug)]
pub struct WebAuth {
    session_secret: String,
    client_id: String,
    client_secret: String,
    /// Public, browser-facing Nextcloud. The authorize redirect goes here.
    nc_base_url: String,
    /// Where *this server* reaches Nextcloud, which is not always the same
    /// address. On the cluster Nextcloud is co-located, so its public name
    /// resolves to the node's own IP and a pod cannot reach it — the request
    /// hairpins and is refused. Pointing this at the in-cluster Service name and
    /// carrying the public host in a `Host:` header keeps Nextcloud's
    /// trusted-domain routing happy while giving the pod an address it can
    /// actually open.
    nc_internal_url: String,
    redirect_uri: String,
    /// Who may enter. Empty means any Nextcloud user this server can see.
    allowed_users: BTreeSet<String>,
}

impl WebAuth {
    /// Build a gate directly, with the Nextcloud addresses left at their
    /// defaults.
    ///
    /// [`from_env`](Self::from_env) is the normal path. This exists because the
    /// alternative for a test is setting process-wide environment variables,
    /// which every other test in the same binary would then be running inside —
    /// and a gate that appears in one test and leaks into the next is exactly
    /// the kind of thing that makes an auth suite untrustworthy.
    pub fn new(
        session_secret: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        allowed_users: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            session_secret: session_secret.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            nc_base_url: DEFAULT_NC_BASE_URL.to_string(),
            nc_internal_url: DEFAULT_NC_BASE_URL.to_string(),
            redirect_uri: DEFAULT_REDIRECT_URI.to_string(),
            allowed_users: allowed_users.into_iter().collect(),
        }
    }

    /// The cookie value that signs `session` in until [`SESSION_TTL`] elapses.
    ///
    /// Public because a test has to be able to arrive already signed in without
    /// standing up a Nextcloud to sign in against.
    pub fn issue_session(&self, session: &Session, now: SystemTime) -> String {
        sign(&self.session_secret, session, now, SESSION_TTL)
    }

    /// Who a cookie says is signed in, if it is authentic and still current.
    ///
    /// The other half of [`issue_session`](Self::issue_session), and public for
    /// the same reason: the properties worth checking about a credential —
    /// that a forged one is refused, that an expired one is refused, that a
    /// payload swapped under a good signature is refused — are properties of
    /// this pair, and testing them through an HTTP round trip would only be able
    /// to say *no* without saying which no.
    pub fn read_session(&self, token: &str, now: SystemTime) -> Option<Session> {
        verify(&self.session_secret, token, now)
    }

    /// Point the gate at a Nextcloud other than the fleet's.
    ///
    /// `internal_url` is where *this server* opens a connection, which is not
    /// always where the browser goes — see the field it sets.
    pub fn with_nextcloud(
        mut self,
        base_url: impl Into<String>,
        internal_url: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        self.nc_base_url = base_url.into();
        self.nc_internal_url = internal_url.into();
        self.redirect_uri = redirect_uri.into();
        self
    }

    /// The name of the cookie [`issue_session`](Self::issue_session) fills.
    pub const COOKIE: &'static str = COOKIE_NAME;

    /// Read the environment, or `None` when sign-in is not configured.
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var(SESSION_SECRET_ENV)
            .ok()
            .filter(|s| !s.is_empty());
        let client_id = std::env::var(CLIENT_ID_ENV).ok().filter(|s| !s.is_empty());
        let client_secret = std::env::var(CLIENT_SECRET_ENV)
            .ok()
            .filter(|s| !s.is_empty());

        let present = [&secret, &client_id, &client_secret].map(Option::is_some);
        if !present.iter().any(|p| *p) {
            return None;
        }
        if !present.iter().all(|p| *p) {
            // Named individually, because the failure someone is debugging here
            // is "I set the secret and the wall did not appear".
            tracing::warn!(
                "sign-in only partly configured ({SESSION_SECRET_ENV}={}, {CLIENT_ID_ENV}={}, \
                 {CLIENT_SECRET_ENV}={}) — the gate stays OFF",
                present[0],
                present[1],
                present[2],
            );
            return None;
        }

        let trim_slash = |s: String| s.trim_end_matches('/').to_string();
        let nc_base_url = trim_slash(
            std::env::var(NC_BASE_URL_ENV).unwrap_or_else(|_| DEFAULT_NC_BASE_URL.to_string()),
        );
        let nc_internal_url = std::env::var(NC_INTERNAL_URL_ENV)
            .ok()
            .map(trim_slash)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| nc_base_url.clone());

        Some(Self {
            session_secret: secret?,
            client_id: client_id?,
            client_secret: client_secret?,
            nc_base_url,
            nc_internal_url,
            redirect_uri: std::env::var(REDIRECT_URI_ENV)
                .unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_string()),
            allowed_users: std::env::var(ALLOWED_USERS_ENV)
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_string)
                .collect(),
        })
    }

    /// The URL, and the `Host` to present, for a call this server makes to
    /// Nextcloud itself.
    ///
    /// Public because it is a statement about the deployment rather than an
    /// implementation detail — and because getting it wrong is a failure that
    /// only appears at the end of a sign-in, as a refused connection, on a
    /// cluster. It cost the fleet's other Nextcloud gate an afternoon.
    pub fn server_call(&self, path: &str) -> (String, Option<String>) {
        let url = format!("{}{path}", self.nc_internal_url);
        if self.nc_internal_url == self.nc_base_url {
            return (url, None);
        }
        // Just the host and port of the public URL, which is what `Host:` is.
        let host = self
            .nc_base_url
            .split_once("://")
            .map_or(self.nc_base_url.as_str(), |(_, rest)| rest)
            .trim_end_matches('/')
            .to_string();
        (url, Some(host))
    }

    /// Whether a signed-in Nextcloud user may enter.
    pub fn permits(&self, user_id: &str) -> bool {
        self.allowed_users.is_empty() || self.allowed_users.contains(user_id)
    }

    /// Where the browser is sent to sign in. Nothing here is secret.
    pub fn authorize_url(&self, state: &str) -> String {
        format!(
            "{}/index.php/apps/oauth2/authorize?client_id={}&response_type=code&redirect_uri={}&state={}",
            self.nc_base_url,
            urlencode(&self.client_id),
            urlencode(&self.redirect_uri),
            urlencode(state),
        )
    }
}

/// Who is signed in. Carried entirely in the cookie; nothing is stored here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Session {
    #[serde(rename = "uid")]
    pub user_id: String,
    #[serde(rename = "name")]
    pub display_name: String,
}

/// The path to return to after signing in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LoginState {
    rt: String,
}

/// A payload plus the moment it stops being true.
#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    #[serde(flatten)]
    body: T,
    exp: u64,
}

/// A `<payload>.<mac>` token: the payload base64url-encoded, and an HMAC of it.
///
/// Stateless on purpose. Verification needs the secret and nothing else, so a
/// restarted pod — or a second replica — honours a cookie it never issued, and
/// there is no session store to back up or to lose.
fn sign<T: Serialize>(secret: &str, body: T, now: SystemTime, ttl: Duration) -> String {
    let exp = now
        .checked_add(ttl)
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    // Not defaulted on failure. An empty payload here would still be signed
    // correctly, so it would verify — and then be read back as a session with no
    // user in it. The two payloads are plain structs of owned strings, so this
    // cannot fail; if that ever stops being true it should stop the program
    // rather than quietly mint an anonymous credential.
    let json = serde_json::to_vec(&Envelope { body, exp })
        .expect("a session payload is a struct of strings and always serialises");
    let encoded = B64.encode(json);
    format!("{encoded}.{}", B64.encode(mac(secret, &encoded)))
}

/// The payload, if the token is authentic and has not expired.
///
/// Every malformed input returns `None` rather than raising: this reads a value
/// an attacker chooses, so the only safe shape is one answer for "no".
fn verify<T: serde::de::DeserializeOwned>(secret: &str, token: &str, now: SystemTime) -> Option<T> {
    let (encoded, presented) = token.split_once('.')?;
    let presented = B64.decode(presented).ok()?;
    // Constant time: a byte-by-byte comparison leaks how much of a forged MAC
    // was right, which is enough to build the rest of it one byte at a time.
    if !constant_time_eq(&mac(secret, encoded), &presented) {
        return None;
    }
    let envelope: Envelope<T> = serde_json::from_slice(&B64.decode(encoded).ok()?).ok()?;
    let seconds = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
    (envelope.exp >= seconds).then_some(envelope.body)
}

fn mac(secret: &str, message: &str) -> Vec<u8> {
    let mut hmac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts a key of any length");
    hmac.update(message.as_bytes());
    hmac.finalize().into_bytes().to_vec()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A redirect target that cannot leave this origin.
///
/// Only a single-slash absolute path survives. `//host`, a scheme, or anything
/// else collapses to `/`, so a crafted `?return_to=` cannot turn signing in into
/// an open redirect — which would make this app a convincing way to send someone
/// somewhere else.
pub fn safe_return_to(raw: Option<&str>) -> String {
    match raw {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_string(),
        _ => "/".to_string(),
    }
}

/// Percent-encode everything that is not unreserved, which is enough for the
/// query values built here and avoids a dependency for one function.
fn urlencode(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The session a request carries, if any.
fn session_from(auth: &WebAuth, headers: &HeaderMap, now: SystemTime) -> Option<Session> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    let token = cookies.split(';').map(str::trim).find_map(|pair| {
        pair.strip_prefix(COOKIE_NAME)
            .and_then(|r| r.strip_prefix('='))
    })?;
    verify(&auth.session_secret, token, now)
}

/// Refuse anything under `/api` that arrives without a session.
///
/// The browser is the only client this app has, so there is no equivalent of a
/// headless device that cannot sign in, and nothing needs an exemption. The
/// health check lives outside `/api` and stays open, which is what lets the
/// cluster probe a pod nobody has signed into.
pub async fn gate(auth: Arc<WebAuth>, request: Request, next: Next) -> Response {
    match session_from(&auth, request.headers(), SystemTime::now()) {
        // 401 rather than a redirect: this is a fetch from a running page, and a
        // 302 to Nextcloud would be followed by the browser and land as an
        // opaque failure. A status the script can read is what raises the wall.
        None => problem(
            StatusCode::UNAUTHORIZED,
            ErrorCode::NotAuthenticated,
            "sign in to continue",
        ),
        Some(session) if !auth.permits(&session.user_id) => problem(
            StatusCode::FORBIDDEN,
            ErrorCode::NotPermitted,
            &format!("{} is not on the list for this app", session.user_id),
        ),
        Some(_) => next.run(request).await,
    }
}

fn problem(status: StatusCode, code: ErrorCode, message: &str) -> Response {
    (
        status,
        Json(crate::error::ErrorBody {
            code,
            message: message.to_string(),
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    ocs: Ocs,
}
#[derive(Debug, Deserialize)]
struct Ocs {
    data: OcsUser,
}
#[derive(Debug, Deserialize)]
struct OcsUser {
    id: String,
    displayname: Option<String>,
}

/// Trade the authorization code for a token, then ask who it belongs to.
async fn identify(auth: &WebAuth, code: &str) -> anyhow::Result<Session> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let (url, host) = auth.server_call("/index.php/apps/oauth2/api/v1/token");
    let mut request = client.post(url).form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", auth.client_id.as_str()),
        ("client_secret", auth.client_secret.as_str()),
        ("redirect_uri", auth.redirect_uri.as_str()),
    ]);
    if let Some(host) = &host {
        request = request.header(header::HOST, host);
    }
    let token: TokenResponse = request.send().await?.error_for_status()?.json().await?;

    let (url, host) = auth.server_call("/ocs/v2.php/cloud/user?format=json");
    let mut request = client
        .get(url)
        .bearer_auth(&token.access_token)
        .header("OCS-APIRequest", "true");
    if let Some(host) = &host {
        request = request.header(header::HOST, host);
    }
    let info: UserInfo = request.send().await?.error_for_status()?.json().await?;

    let id = info.ocs.data.id;
    let display_name = info.ocs.data.displayname.unwrap_or_else(|| id.clone());
    Ok(Session {
        user_id: id,
        display_name,
    })
}

/// The routes sign-in adds. Only mounted when the gate is up.
///
/// Generic in the router's state because none of these touch it — they need the
/// Nextcloud credentials and nothing else — and that keeps this module free of
/// any knowledge of what the rest of the app is holding.
pub fn routes<S: Clone + Send + Sync + 'static>(auth: Arc<WebAuth>) -> Router<S> {
    let login_auth = auth.clone();
    let callback_auth = auth.clone();
    let me_auth = auth;

    Router::new()
        .route(
            "/login",
            get(move |Query(query): Query<LoginQuery>| {
                let auth = login_auth.clone();
                async move {
                    let state = sign(
                        &auth.session_secret,
                        LoginState {
                            rt: safe_return_to(query.return_to.as_deref()),
                        },
                        SystemTime::now(),
                        STATE_TTL,
                    );
                    redirect(&auth.authorize_url(&state), None)
                }
            }),
        )
        .route(
            "/auth/callback",
            get(move |Query(query): Query<CallbackQuery>| {
                let auth = callback_auth.clone();
                async move { callback(auth, query).await }
            }),
        )
        .route(
            "/logout",
            post(move || async move {
                // Expired rather than deleted by name alone, so a browser that
                // ignores an empty value still drops it.
                redirect(
                    "/",
                    Some(format!(
                        "{COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax; Secure"
                    )),
                )
            }),
        )
        .route(
            "/api/me",
            get(move |headers: HeaderMap| {
                let auth = me_auth.clone();
                async move {
                    // The gate has already run, so a session exists — but this
                    // reads it again rather than trusting that, because the day
                    // someone mounts this route outside the gate is the day
                    // "trust me" becomes an unauthenticated identity endpoint.
                    match session_from(&auth, &headers, SystemTime::now()) {
                        Some(session) => Json(session).into_response(),
                        None => problem(
                            StatusCode::UNAUTHORIZED,
                            ErrorCode::NotAuthenticated,
                            "sign in to continue",
                        ),
                    }
                }
            }),
        )
}

async fn callback(auth: Arc<WebAuth>, query: CallbackQuery) -> Response {
    let now = SystemTime::now();
    let Some(state) = query
        .state
        .as_deref()
        .and_then(|s| verify::<LoginState>(&auth.session_secret, s, now))
    else {
        return problem(
            StatusCode::FORBIDDEN,
            ErrorCode::BadLoginState,
            "that sign-in link has expired — start again",
        );
    };
    let Some(code) = query.code.filter(|c| !c.is_empty()) else {
        return problem(
            StatusCode::BAD_REQUEST,
            ErrorCode::NoAuthorizationCode,
            "Nextcloud returned no authorization code",
        );
    };

    let session = match identify(&auth, &code).await {
        Ok(session) => session,
        Err(why) => {
            // Logged rather than returned: the detail is about this server's
            // conversation with Nextcloud and means nothing to the person.
            tracing::error!("nextcloud sign-in failed: {why:#}");
            return problem(
                StatusCode::BAD_GATEWAY,
                ErrorCode::SignInFailed,
                "could not complete the sign-in with Nextcloud",
            );
        }
    };
    if !auth.permits(&session.user_id) {
        return problem(
            StatusCode::FORBIDDEN,
            ErrorCode::NotPermitted,
            &format!("{} is not on the list for this app", session.user_id),
        );
    }

    let token = sign(&auth.session_secret, session, now, SESSION_TTL);
    redirect(
        &safe_return_to(Some(&state.rt)),
        Some(format!(
            "{COOKIE_NAME}={token}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax; Secure",
            SESSION_TTL.as_secs()
        )),
    )
}

/// A 302 with an optional cookie.
///
/// `Secure` is set on every cookie here, unlike the fleet's other Nextcloud
/// gate, because this app is served over TLS. That means sign-in does not work
/// over plain http — which is the point: a session cookie that travels in clear
/// text on a shared network is the thing the gate was raised against.
fn redirect(location: &str, set_cookie: Option<String>) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location);
    if let Some(cookie) = set_cookie {
        response = response.header(header::SET_COOKIE, cookie);
    }
    response
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
