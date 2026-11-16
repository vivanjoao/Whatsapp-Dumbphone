use crate::{
    auth::{self, Auth},
    config::Config,
    domain::{Gateway, GatewayError, valid_id, valid_text},
    store::Store,
    views,
};
use axum::{
    Router,
    extract::{ConnectInfo, DefaultBodyLimit, Form, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct App {
    pub config: Arc<Config>,
    pub auth: Auth,
    pub store: Store,
    pub gateway: Arc<dyn Gateway>,
    pub requests: Arc<Semaphore>,
    pub sends: Arc<Semaphore>,
}
impl App {
    pub fn new(config: Config, store: Store, gateway: Arc<dyn Gateway>) -> Self {
        Self {
            auth: Auth::new(&config.secret, &config.password, config.token.as_deref()),
            store,
            gateway,
            requests: Arc::new(Semaphore::new(config.max_requests)),
            sends: Arc::new(Semaphore::new(config.max_sends)),
            config: Arc::new(config),
        }
    }
}
pub fn router(app: App) -> Router {
    Router::new()
        .route("/", get(chats))
        .route("/chat/{id}", get(conversation))
        .route("/login", post(login))
        .route("/send", post(send))
        .route("/health/live", get(|| async { "ok" }))
        .route("/health/ready", get(ready))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}
async fn guard(State(app): State<App>, req: Request, next: Next) -> Response {
    let response = match app.requests.clone().try_acquire_owned() {
        Ok(_permit) => tokio::time::timeout(Duration::from_secs(30), next.run(req))
            .await
            .unwrap_or_else(|_| {
                error(
                    StatusCode::GATEWAY_TIMEOUT,
                    "Request timed out. Check the conversation before resending.",
                )
            }),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "Server busy. Try later."),
    };
    let mut response = response;
    for (key, value) in [
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        (
            "content-security-policy",
            "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    ] {
        response.headers_mut().insert(
            header::HeaderName::from_static(key),
            header::HeaderValue::from_static(value),
        );
    }
    response
}
fn error(status: StatusCode, message: &'static str) -> Response {
    (status, message).into_response()
}
fn internal() -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Request could not be completed.",
    )
}
fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "WhatsApp is unavailable. Try later.",
    )
}
fn gateway_error(err: GatewayError) -> Response {
    match err {
        GatewayError::NotFound => error(StatusCode::NOT_FOUND, "Conversation not found."),
        _ => unavailable(),
    }
}
fn cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|p| p.trim().split_once('='))
        .find(|(key, _)| *key == "gateway_session")
        .map(|(_, value)| value.to_owned())
        .filter(|value| value.len() <= 256)
}
fn with_cookie(app: &App, mut response: Response, id: &str, age: i64) -> Response {
    let value = format!(
        "gateway_session={id}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{}",
        if app.config.secure_cookie {
            "; Secure"
        } else {
            ""
        }
    );
    if let Ok(value) = value.parse() {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}
#[derive(Deserialize, Default)]
struct Bookmark {
    token: Option<String>,
    #[serde(default)]
    view: views::Layout,
}
enum Principal {
    Session(String),
    Token(String),
}
impl Principal {
    fn context(&self) -> String {
        match self {
            Self::Session(id) => format!("session:{id}"),
            Self::Token(t) => format!("token:{t}"),
        }
    }
    fn legacy(&self) -> Option<&str> {
        match self {
            Self::Token(t) => Some(t),
            _ => None,
        }
    }
}
async fn authorize(
    app: &App,
    headers: &HeaderMap,
    token: Option<&str>,
    path: &str,
    layout: views::Layout,
) -> Result<Principal, Box<Response>> {
    if let (Some(expected), Some(token)) = (&app.config.token, token)
        && auth::equal(expected, token)
    {
        if app.config.legacy_token_urls {
            return Ok(Principal::Token(token.to_owned()));
        }
        let id = format!("s.{}", auth::random());
        app.store
            .login(&id, &app.auth.epoch)
            .await
            .map_err(|_| Box::new(internal()))?;
        return Err(Box::new(with_cookie(
            app,
            Redirect::to(
                &views::Context {
                    token: None,
                    layout,
                }
                .href(path),
            )
            .into_response(),
            &id,
            30 * 86400,
        )));
    }
    if let Some(id) = cookie(headers)
        && id.starts_with("s.")
        && app
            .store
            .session(&id, &app.auth.epoch)
            .await
            .map_err(|_| Box::new(internal()))?
    {
        return Ok(Principal::Session(id));
    }
    let id = cookie(headers)
        .filter(|id| {
            id.strip_prefix("p.")
                .is_some_and(|value| app.auth.verify("prelogin", value))
        })
        .unwrap_or_else(|| format!("p.{}", app.auth.issue("prelogin")));
    let csrf = app.auth.issue(&format!("login:{id}"));
    Err(Box::new(with_cookie(
        app,
        (
            StatusCode::UNAUTHORIZED,
            Html(views::login(
                &csrf,
                views::Context {
                    token: None,
                    layout,
                },
            )),
        )
            .into_response(),
        &id,
        3600,
    )))
}

async fn ready(State(app): State<App>) -> Response {
    if app.gateway.ready() {
        "ready".into_response()
    } else {
        unavailable()
    }
}
async fn chats(
    State(app): State<App>,
    Query(query): Query<Bookmark>,
    headers: HeaderMap,
) -> Response {
    let principal = match authorize(&app, &headers, query.token.as_deref(), "/", query.view).await {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !app.gateway.ready() {
        return unavailable();
    }
    match app.gateway.chats().await {
        Ok(chats) => Html(views::chats(
            &chats,
            views::Context {
                token: principal.legacy(),
                layout: query.view,
            },
        ))
        .into_response(),
        Err(e) => gateway_error(e),
    }
}
async fn conversation(
    State(app): State<App>,
    Path(id): Path<String>,
    Query(query): Query<Bookmark>,
    headers: HeaderMap,
) -> Response {
    if !valid_id(&id) {
        return error(StatusCode::BAD_REQUEST, "Invalid conversation.");
    }
    let principal = match authorize(
        &app,
        &headers,
        query.token.as_deref(),
        &format!("/chat/{}", views::url(&id)),
        query.view,
    )
    .await
    {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !app.gateway.ready() {
        return unavailable();
    }
    let conversation = match app.gateway.conversation(&id).await {
        Ok(c) => c,
        Err(e) => return gateway_error(e),
    };
    let read_failed = if conversation.chat.unread != 0 {
        !matches!(
            tokio::time::timeout(Duration::from_secs(3), app.gateway.mark_read(&conversation))
                .await,
            Ok(Ok(()))
        )
    } else {
        false
    };
    let csrf = app.auth.issue(&format!(
        "send:{}:{}",
        principal.context(),
        conversation.chat.id
    ));
    // Sidebar is a bounded local query. Its failure must not hide the open chat.
    // Explicit compact mode avoids this query and its extra HTML on old phones.
    let (chats, sidebar_failed) = if query.view == views::Layout::Compact {
        (Vec::new(), false)
    } else {
        match app.gateway.chats().await {
            Ok(chats) => (chats, false),
            Err(_) => (Vec::new(), true),
        }
    };
    // A fresh public nonce forces Refresh to perform a GET even when the
    // current URL already points at the end-of-history fragment.
    let refresh_id = auth::random();
    Html(views::conversation(
        &conversation,
        &chats,
        &csrf,
        views::Context {
            token: principal.legacy(),
            layout: query.view,
        },
        read_failed,
        sidebar_failed,
        &refresh_id,
    ))
    .into_response()
}
#[derive(Deserialize)]
struct Login {
    pw: String,
    #[serde(default)]
    csrf: String,
    #[serde(default)]
    view: views::Layout,
}
async fn login(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<Login>,
) -> Response {
    // Never trust a caller-supplied X-Forwarded-For header. Proxies share one bucket.
    match app.store.attempt(&peer.ip().to_string()).await {
        Ok(true) => {}
        Ok(false) => {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many attempts. Try in 15 minutes.",
            );
        }
        Err(_) => return internal(),
    }
    let Some(id) = cookie(&headers) else {
        return error(StatusCode::FORBIDDEN, "Reload the login page.");
    };
    let valid_context = id
        .strip_prefix("p.")
        .is_some_and(|s| app.auth.verify("prelogin", s));
    if !valid_context || !app.auth.verify(&format!("login:{id}"), &form.csrf) {
        return error(
            StatusCode::FORBIDDEN,
            "Invalid or expired form. Reload the page.",
        );
    }
    if !auth::equal(&form.pw, &app.config.password) {
        return error(
            StatusCode::UNAUTHORIZED,
            "Invalid password. Reload the login page.",
        );
    }
    let new_id = format!("s.{}", auth::random());
    if app.store.login(&new_id, &app.auth.epoch).await.is_err() {
        return internal();
    }
    let target = views::Context {
        token: None,
        layout: form.view,
    }
    .href("/");
    with_cookie(
        &app,
        Redirect::to(&target).into_response(),
        &new_id,
        30 * 86400,
    )
}
#[derive(Deserialize)]
struct SendForm {
    to: String,
    msg: String,
    #[serde(default)]
    csrf: String,
    token: Option<String>,
    #[serde(default)]
    view: views::Layout,
}
async fn send(State(app): State<App>, headers: HeaderMap, Form(form): Form<SendForm>) -> Response {
    let principal = match authorize(&app, &headers, form.token.as_deref(), "/", form.view).await {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !valid_id(&form.to) || !valid_text(&form.msg) {
        return error(
            StatusCode::BAD_REQUEST,
            "Invalid recipient or message (maximum 4096 characters).",
        );
    }
    if !app.auth.verify(
        &format!("send:{}:{}", principal.context(), form.to),
        &form.csrf,
    ) {
        return error(
            StatusCode::FORBIDDEN,
            "Invalid or expired form. Reload the page.",
        );
    }
    if !app.gateway.ready() {
        return unavailable();
    }
    let Ok(_permit) = app.sends.clone().try_acquire_owned() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Too many sends in progress. Try later.",
        );
    };
    match app.store.reserve(&form.csrf).await {
        Ok(true) => {}
        Ok(false) => {
            return error(
                StatusCode::CONFLICT,
                "Form already submitted. Check the conversation before sending again.",
            );
        }
        Err(_) => return internal(),
    }
    let sent = matches!(
        tokio::time::timeout(
            Duration::from_secs(20),
            app.gateway.send(&form.to, &form.msg)
        )
        .await,
        Ok(Ok(()))
    );
    if app.store.finish(&form.csrf, sent).await.is_err() {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Send status could not be stored. Check the conversation before resending.",
        );
    }
    if !sent {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Delivery could not be confirmed. Check the conversation before sending again.",
        );
    }
    let target = views::Context {
        token: principal.legacy(),
        layout: form.view,
    }
    .href(&format!("/chat/{}", views::url(&form.to)));
    let target = format!("{target}#messages-end");
    Redirect::to(&target).into_response()
}
