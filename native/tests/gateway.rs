use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use dumbphone_gateway::{
    auth::{self, Auth},
    config::Config,
    domain::*,
    http::{self, App},
    store::Store,
    views,
};
use http_body_util::BodyExt;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tower::ServiceExt;

const TOKEN: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ID: &str = "123@s.whatsapp.net";
struct Fake {
    ready: AtomicBool,
    fail: AtomicBool,
    sends: AtomicUsize,
    reads: AtomicUsize,
    chat_queries: AtomicUsize,
    fail_sidebar: AtomicBool,
}
impl Fake {
    fn new() -> Self {
        Self {
            ready: AtomicBool::new(true),
            fail: AtomicBool::new(false),
            sends: AtomicUsize::new(0),
            reads: AtomicUsize::new(0),
            chat_queries: AtomicUsize::new(0),
            fail_sidebar: AtomicBool::new(false),
        }
    }
}
#[async_trait]
impl Gateway for Fake {
    fn ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
    async fn chats(&self) -> Result<Vec<Chat>, GatewayError> {
        self.chat_queries.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) || self.fail_sidebar.load(Ordering::SeqCst) {
            return Err(GatewayError::Failed);
        }
        Ok(vec![
            Chat {
                id: ID.into(),
                name: "<script>private</script>".into(),
                unread: 1
            };
            25
        ])
    }
    async fn conversation(&self, id: &str) -> Result<Conversation, GatewayError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(GatewayError::Failed);
        }
        Ok(Conversation {
            chat: Chat {
                id: id.into(),
                name: "\"<b>Group</b>".into(),
                unread: 1,
            },
            messages: vec![Message {
                id: "message-id".into(),
                sender_id: ID.into(),
                sender_name: "<img src=x onerror=x>".into(),
                text: "Olá 🦀 <script>alert(1)</script> & \"".into(),
                from_me: false,
                time: "12:00".into(),
                media: Some("<audio>".into()),
            }],
        })
    }
    async fn mark_read(&self, _: &Conversation) -> Result<(), GatewayError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn send(&self, _: &str, _: &str) -> Result<(), GatewayError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            Err(GatewayError::Failed)
        } else {
            Ok(())
        }
    }
}
fn config() -> Config {
    Config::from_values(HashMap::from([
        ("PASSWORD".into(), "unique-test-password".into()),
        ("SESSION_SECRET".into(), "a".repeat(64)),
        ("ACCESS_TOKEN".into(), TOKEN.into()),
    ]))
    .unwrap()
}
struct Fixture {
    app: App,
    fake: Arc<Fake>,
    _dir: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("web.db")).await.unwrap();
        let fake = Arc::new(Fake::new());
        Self {
            app: App::new(config(), store, fake.clone()),
            fake,
            _dir: dir,
        }
    }
    fn router(&self) -> Router {
        http::router(self.app.clone()).layer(Extension(ConnectInfo(
            "127.0.0.1:1234".parse::<SocketAddr>().unwrap(),
        )))
    }
}
struct Reply {
    status: StatusCode,
    text: String,
    cookie: Option<String>,
    location: Option<String>,
    headers: axum::http::HeaderMap,
}
async fn call(
    router: Router,
    method: &str,
    path: &str,
    form: Option<&str>,
    cookie: Option<&str>,
) -> Reply {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    if form.is_some() {
        request = request.header("content-type", "application/x-www-form-urlencoded");
    }
    let response = router
        .oneshot(
            request
                .body(Body::from(form.unwrap_or_default().to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let cookie = headers
        .get("set-cookie")
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned());
    let location = headers
        .get("location")
        .map(|v| v.to_str().unwrap().to_owned());
    let text = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    Reply {
        status,
        text,
        cookie,
        location,
        headers,
    }
}
fn csrf(html: &str) -> String {
    html.split("name=\"csrf\" value=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .into()
}
fn form(values: &[(&str, &str)]) -> String {
    values
        .iter()
        .map(|(k, v)| format!("{}={}", views::url(k), views::url(v)))
        .collect::<Vec<_>>()
        .join("&")
}
async fn send_form(f: &Fixture) -> String {
    let page = call(
        f.router(),
        "GET",
        &format!("/chat/{ID}?token={TOKEN}"),
        None,
        None,
    )
    .await;
    assert_eq!(page.status, 200);
    form(&[
        ("token", TOKEN),
        ("csrf", &csrf(&page.text)),
        ("to", ID),
        ("msg", "Olá 🦀"),
    ])
}
#[test]
fn configuration_and_validation() {
    assert!(Config::from_values(HashMap::new()).is_err());
    for id in [ID, "123@c.us", "123-456@g.us", "123@lid"] {
        assert!(valid_id(id));
    }
    for id in [
        "",
        "../admin",
        "https://example.com",
        "x@s.whatsapp.net",
        "123@evil.net",
        "1--2@g.us",
    ] {
        assert!(!valid_id(id));
    }
    assert!(!valid_text("  "));
    assert!(valid_text(&"🦀".repeat(4096)));
    assert!(!valid_text(&"x".repeat(4097)));
    let mut values = HashMap::from([
        ("PASSWORD".into(), "REPLACE_WITH_A_PASSWORD".into()),
        ("SESSION_SECRET".into(), "a".repeat(64)),
    ]);
    assert!(Config::from_values(values.clone()).is_err());
    values.insert("PASSWORD".into(), "long-test-password".into());
    values.insert("COOKIE_SECURE".into(), "maybe".into());
    assert!(Config::from_values(values).is_err());
}
#[test]
fn signatures_bind_context_expiry_and_credentials() {
    let auth = Auth::new("secret", "password", Some(TOKEN));
    let token = auth.issue("context");
    assert!(auth.verify("context", &token));
    assert!(!auth.verify("other", &token));
    assert!(!auth.verify("context", ""));
    assert!(!auth.verify("context", &auth.issue_at("context", auth::now() - 4000)));
    assert!(!auth.verify("context", &auth.issue_at("context", auth::now() + 60)));
    assert!(!Auth::new("secret", "changed", Some(TOKEN)).verify("context", &token));
    assert!(!auth::equal("", ""));
    assert!(auth::equal("match", "match"));
}
#[tokio::test]
async fn unauthenticated_token_types_and_disabled_mode() {
    let mut f = Fixture::new().await;
    for path in ["/", "/?token=", "/?token=bad", "/?token[]=x"] {
        assert_eq!(call(f.router(), "GET", path, None, None).await.status, 401);
    }
    let res = call(f.router(), "GET", &format!("/?token={TOKEN}"), None, None).await;
    assert_eq!(res.status, 200);
    assert_eq!(res.headers["cache-control"], "no-store");
    assert_eq!(res.headers["referrer-policy"], "no-referrer");
    f.app.config = Arc::new(Config {
        token: None,
        ..config()
    });
    assert_eq!(
        call(f.router(), "GET", &format!("/?token={TOKEN}"), None, None)
            .await
            .status,
        401
    );
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn content_is_escaped_and_legacy_navigation_works() {
    let f = Fixture::new().await;
    let page = call(
        f.router(),
        "GET",
        &format!("/chat/{ID}?token={TOKEN}"),
        None,
        None,
    )
    .await;
    assert_eq!(page.status, 200);
    assert!(page.text.contains("Olá 🦀"));
    assert!(page.text.contains("&lt;script&gt;"));
    assert!(!page.text.contains("<script>"));
    assert!(!page.text.contains("<img "));
    assert!(
        page.text
            .contains(&format!("name=\"token\" value=\"{TOKEN}\""))
    );
    assert_eq!(f.fake.reads.load(Ordering::SeqCst), 1);
    let list = call(f.router(), "GET", &format!("/?token={TOKEN}"), None, None).await;
    assert_eq!(list.text.matches(r#"class="chat-link""#).count(), 20);
}
#[tokio::test]
async fn login_csrf_session_rotation_and_rate_limit() {
    let f = Fixture::new().await;
    let page = call(f.router(), "GET", "/", None, None).await;
    let old = page.cookie.as_deref();
    let data = form(&[("pw", &config().password), ("csrf", &csrf(&page.text))]);
    assert_eq!(
        call(f.router(), "POST", "/login", Some(&data), None)
            .await
            .status,
        403
    );
    let missing = form(&[("pw", &config().password)]);
    assert_eq!(
        call(f.router(), "POST", "/login", Some(&missing), old)
            .await
            .status,
        403
    );
    let login = call(f.router(), "POST", "/login", Some(&data), old).await;
    assert_eq!(login.status, 303);
    assert_ne!(login.cookie, page.cookie);
    assert_eq!(
        call(f.router(), "GET", "/", None, login.cookie.as_deref())
            .await
            .status,
        200
    );
    let denied = form(&[("pw", "wrong"), ("csrf", &csrf(&page.text))]);
    assert_eq!(
        call(f.router(), "POST", "/login", Some(&denied), old)
            .await
            .status,
        401
    );
    assert_eq!(
        call(f.router(), "POST", "/login", Some(&denied), old)
            .await
            .status,
        401
    );
    assert_eq!(
        call(f.router(), "POST", "/login", Some(&denied), old)
            .await
            .status,
        429
    );
}
#[tokio::test]
async fn send_once_even_for_concurrent_duplicate_submissions() {
    let f = Fixture::new().await;
    let data = send_form(&f).await;
    let (a, b) = tokio::join!(
        call(f.router(), "POST", "/send", Some(&data), None),
        call(f.router(), "POST", "/send", Some(&data), None)
    );
    assert!([a.status, b.status].contains(&StatusCode::SEE_OTHER));
    assert!([a.status, b.status].contains(&StatusCode::CONFLICT));
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn invalid_input_csrf_and_oversized_body_never_send() {
    let f = Fixture::new().await;
    let data = send_form(&f).await;
    for bad in [
        data.replace("msg=Ol%C3%A1%20%F0%9F%A6%80", "msg="),
        data.replace("csrf=", "csrf=bad"),
        data.replace("to=123%40s.whatsapp.net", "to=1%40evil.net"),
        format!("{data}&to=456%40lid"),
    ] {
        assert!(
            call(f.router(), "POST", "/send", Some(&bad), None)
                .await
                .status
                .is_client_error()
        );
    }
    let oversized = format!("msg={}", "x".repeat(66000));
    assert_eq!(
        call(f.router(), "POST", "/send", Some(&oversized), None)
            .await
            .status,
        413
    );
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn failure_is_not_retried_and_restart_preserves_deduplication() {
    let mut f = Fixture::new().await;
    let data = send_form(&f).await;
    f.fake.fail.store(true, Ordering::SeqCst);
    let res = call(f.router(), "POST", "/send", Some(&data), None).await;
    assert_eq!(res.status, 503);
    assert!(res.text.contains("could not be confirmed"));
    f.app.store = Store::open(&f._dir.path().join("web.db")).await.unwrap();
    assert_eq!(
        call(f.router(), "POST", "/send", Some(&data), None)
            .await
            .status,
        409
    );
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 1);
    assert_eq!(
        call(
            f.router(),
            "GET",
            &format!("/chat/{ID}?token={TOKEN}"),
            None,
            None
        )
        .await
        .status,
        503
    );
}
#[tokio::test]
async fn cookie_exchange_persistence_and_revocation() {
    let mut f = Fixture::new().await;
    f.app.config = Arc::new(Config {
        legacy_token_urls: false,
        secure_cookie: true,
        ..config()
    });
    let exchange = call(f.router(), "GET", &format!("/?token={TOKEN}"), None, None).await;
    assert_eq!(exchange.status, 303);
    assert_eq!(exchange.location.as_deref(), Some("/"));
    let cookie = exchange.cookie.as_deref();
    assert!(
        exchange.headers["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Secure")
    );
    f.app.store = Store::open(&f._dir.path().join("web.db")).await.unwrap();
    let page = call(f.router(), "GET", "/", None, cookie).await;
    assert_eq!(page.status, 200);
    assert!(!page.text.contains("token="));
    let chat = call(f.router(), "GET", &format!("/chat/{ID}"), None, cookie).await;
    let data = form(&[("to", ID), ("msg", "hello"), ("csrf", &csrf(&chat.text))]);
    assert_eq!(
        call(f.router(), "POST", "/send", Some(&data), cookie)
            .await
            .status,
        303
    );
    let changed = Config {
        password: "changed-password".into(),
        ..config()
    };
    f.app = App::new(changed, f.app.store.clone(), f.fake.clone());
    assert_eq!(call(f.router(), "GET", "/", None, cookie).await.status, 401);
}
#[tokio::test]
async fn admission_limits_and_readiness() {
    let f = Fixture::new().await;
    let permit = f.app.requests.clone().acquire_many_owned(64).await.unwrap();
    assert_eq!(call(f.router(), "GET", "/", None, None).await.status, 503);
    drop(permit);
    let data = send_form(&f).await;
    let permit = f.app.sends.clone().acquire_many_owned(4).await.unwrap();
    assert_eq!(
        call(f.router(), "POST", "/send", Some(&data), None)
            .await
            .status,
        503
    );
    drop(permit);
    assert_eq!(f.fake.sends.load(Ordering::SeqCst), 0);
    f.fake.ready.store(false, Ordering::SeqCst);
    assert_eq!(
        call(f.router(), "GET", "/health/ready", None, None)
            .await
            .status,
        503
    );
    assert_eq!(
        call(f.router(), "GET", "/health/live", None, None)
            .await
            .status,
        200
    );
    assert_eq!(
        call(f.router(), "POST", "/send", Some(&data), None)
            .await
            .status,
        503
    );
}

#[tokio::test]
async fn expired_sessions_and_pending_submissions_survive_restart_safely() {
    use diesel::prelude::*;
    let mut f = Fixture::new().await;
    let id = "s.session-test";
    f.app.store.login(id, &f.app.auth.epoch).await.unwrap();
    assert!(f.app.store.session(id, &f.app.auth.epoch).await.unwrap());
    f.app.store.reserve("pending-form").await.unwrap();
    f.app.store = Store::open(&f._dir.path().join("web.db")).await.unwrap();
    assert!(!f.app.store.reserve("pending-form").await.unwrap());
    let mut conn =
        SqliteConnection::establish(f._dir.path().join("web.db").to_str().unwrap()).unwrap();
    diesel::sql_query("UPDATE sessions SET expires=0")
        .execute(&mut conn)
        .unwrap();
    assert!(!f.app.store.session(id, &f.app.auth.epoch).await.unwrap());
    f.app.store.prune().await.unwrap();
}
#[test]
fn escaping_covers_attribute_delimiters_and_urls() {
    assert_eq!(views::escape("<&\"'>"), "&lt;&amp;&quot;&#39;&gt;");
    assert_eq!(
        views::url("123@lid\"&token=bad"),
        "123%40lid%22%26token%3Dbad"
    );
}
#[test]
fn binary_validates_config_without_network_or_sensitive_output() {
    let dir = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_dumbphone-gateway");
    let output = std::process::Command::new(binary)
        .arg("--check-config")
        .current_dir(dir.path())
        .env_clear()
        .env("PASSWORD", "unique-test-password")
        .env("SESSION_SECRET", "a".repeat(64))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("no connection started"));
    assert!(!dir.path().join("data").exists());
    let invalid = std::process::Command::new(binary)
        .arg("--check-config")
        .current_dir(dir.path())
        .env_clear()
        .output()
        .unwrap();
    assert!(!invalid.status.success());
}

#[test]
fn second_process_cannot_open_the_same_account_directory() {
    use fs2::FileExt;
    let dir = tempfile::tempdir().unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.path().join("gateway.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_dumbphone-gateway"))
        .current_dir(dir.path())
        .env_clear()
        .env("PASSWORD", "unique-test-password")
        .env("SESSION_SECRET", "a".repeat(64))
        .env("DATA_DIR", dir.path())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Another gateway owns this DATA_DIR"));
    assert!(!dir.path().join("whatsapp.db").exists());
}

#[tokio::test]
async fn compact_layout_preserves_bookmark_and_skips_sidebar_query() {
    let f = Fixture::new().await;
    let path = format!("/chat/{ID}?token={TOKEN}&view=compact");
    let compact = call(f.router(), "GET", &path, None, None).await;
    assert_eq!(compact.status, 200);
    assert_eq!(f.fake.chat_queries.load(Ordering::SeqCst), 0);
    assert!(!compact.text.contains("@media screen"));
    assert!(!compact.text.contains("class=\"chat-link\""));
    assert!(
        compact
            .text
            .contains(&format!("href=\"/?token={TOKEN}&amp;view=compact\""))
    );
    assert!(compact.text.contains("name=\"view\" value=\"compact\""));
    let data = form(&[
        ("token", TOKEN),
        ("view", "compact"),
        ("to", ID),
        ("msg", "From a small phone"),
        ("csrf", &csrf(&compact.text)),
    ]);
    let send = call(f.router(), "POST", "/send", Some(&data), None).await;
    assert_eq!(send.status, 303);
    assert_eq!(
        send.location.as_deref(),
        Some(format!("/chat/123%40s.whatsapp.net?token={TOKEN}&view=compact").as_str())
    );
    let responsive = call(
        f.router(),
        "GET",
        &format!("/chat/{ID}?token={TOKEN}"),
        None,
        None,
    )
    .await;
    assert_eq!(responsive.status, 200);
    assert!(responsive.text.contains("@media screen"));
    assert!(responsive.text.contains("aria-current=\"page\""));
    assert!(compact.text.len() < responsive.text.len() / 2);
}
#[tokio::test]
async fn sidebar_failure_does_not_hide_messages_or_send_form() {
    let f = Fixture::new().await;
    f.fake.fail_sidebar.store(true, Ordering::SeqCst);
    let page = call(
        f.router(),
        "GET",
        &format!("/chat/{ID}?token={TOKEN}"),
        None,
        None,
    )
    .await;
    assert_eq!(page.status, 200);
    assert!(page.text.contains("The chat list could not be loaded"));
    assert!(page.text.contains("Olá 🦀"));
    assert!(page.text.contains("action=\"/send\""));
}
#[tokio::test]
async fn compact_login_and_cookie_exchange_keep_the_selected_layout() {
    let mut f = Fixture::new().await;
    let page = call(f.router(), "GET", "/?view=compact", None, None).await;
    assert_eq!(page.status, 401);
    assert!(!page.text.contains("@media screen"));
    let data = form(&[
        ("pw", &config().password),
        ("view", "compact"),
        ("csrf", &csrf(&page.text)),
    ]);
    let login = call(
        f.router(),
        "POST",
        "/login",
        Some(&data),
        page.cookie.as_deref(),
    )
    .await;
    assert_eq!(login.location.as_deref(), Some("/?view=compact"));
    f.app.config = Arc::new(Config {
        legacy_token_urls: false,
        ..config()
    });
    let exchange = call(
        f.router(),
        "GET",
        &format!("/chat/{ID}?token={TOKEN}&view=compact"),
        None,
        None,
    )
    .await;
    assert_eq!(
        exchange.location.as_deref(),
        Some("/chat/123%40s.whatsapp.net?view=compact")
    );
    assert!(!exchange.location.unwrap().contains(TOKEN));
}
