use axum::{
    extract::{Form, OriginalUri, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use ncut_core::{Engine, Status};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Clone)]
struct Fixture {
    mode: &'static str,
    port: u16,
    online: Arc<AtomicBool>,
    visits: Arc<AtomicUsize>,
    submitted: Arc<AtomicBool>,
    keepalive_visits: Arc<AtomicUsize>,
    verification_attempts: Arc<AtomicUsize>,
}

impl Fixture {
    fn campus_form(&self) -> bool {
        matches!(
            self.mode,
            "direct" | "campus_http" | "campus_js" | "foreign_redirect"
        )
    }
}

async fn probe(State(state): State<Fixture>) -> Response {
    if state.online.load(Ordering::SeqCst) {
        if state.mode == "direct"
            && state.submitted.load(Ordering::SeqCst)
            && state.verification_attempts.fetch_add(1, Ordering::SeqCst) < 2
        {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        return StatusCode::NO_CONTENT.into_response();
    }
    match state.mode {
        "unrelated" => "<title>Other network</title>".into_response(),
        "large" => "x".repeat(300_000).into_response(),
        "direct" => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        "js" => "<script>window.location = '/fgtauth?from-url';</script>".into_response(),
        "campus_http" => Redirect::temporary("/login?from-url").into_response(),
        "campus_js" => {
            "<script>window.location.replace('/login?from-url');</script>".into_response()
        }
        "foreign_redirect" => {
            Redirect::temporary(&format!("http://localhost:{}/fgtauth?from-url", state.port))
                .into_response()
        }
        _ => Redirect::temporary("/fgtauth?from-url").into_response(),
    }
}
async fn campus_portal(State(state): State<Fixture>, OriginalUri(uri): OriginalUri) -> Response {
    let query = uri.query().unwrap();
    if matches!(state.mode, "direct" | "foreign_redirect") {
        assert_eq!(query.len(), 16);
        assert!(query.bytes().all(|byte| byte.is_ascii_hexdigit()));
    } else {
        assert_eq!(query, "from-url");
    }
    let visit = state.visits.fetch_add(1, Ordering::SeqCst) + 1;
    // Field order, names and action come from the live NCUT /login form.
    format!(
        "<title>校園網路認證 | 國立勤益科技大學</title>
         <form id='loginForm' action='/' method='post'>
         <input type='hidden' name='4Tredir' value='http://127.0.0.1:{}/login?{query}'>
         <input type='hidden' name='magic' value='{visit:016x}'>
         <input type='text' name='username' required>
         <input type='password' name='password' required></form>",
        state.port
    )
    .into_response()
}

async fn portal(State(state): State<Fixture>) -> Response {
    let visit = state.visits.fetch_add(1, Ordering::SeqCst) + 1;
    let title = if state.mode == "wrong_title" {
        "Other"
    } else {
        "勤益科技大學"
    };
    let action = if state.mode == "foreign_form" {
        "http://example.com/auth"
    } else {
        "/auth"
    };
    let body = format!(
        "<title>{title}</title><form method='post' action='{action}'>
         <input name='username'><input type='password' name='password'>
         <input name='magic' value='token-{visit}&amp;special'>
         <input name='4Tredir' value='http://test.invalid/destination?x=1&amp;y=2'></form>"
    );
    ([("set-cookie", "session=fixture; Path=/; HttpOnly")], body).into_response()
}
async fn submit(
    State(state): State<Fixture>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    state.submitted.store(true, Ordering::SeqCst);
    assert_eq!(form["username"], "s+中文&= 123");
    assert_eq!(form["password"], "secret&=+\"'\\ 密碼");
    let visit = state.visits.load(Ordering::SeqCst);
    if state.campus_form() {
        let referer = headers["referer"].to_str().unwrap();
        assert!(referer.starts_with("http://127.0.0.1:"));
        assert_eq!(form["4Tredir"], referer);
        assert_eq!(form["magic"], format!("{visit:016x}"));
    } else {
        assert!(headers["cookie"]
            .to_str()
            .unwrap()
            .contains("session=fixture"));
        assert_eq!(form["magic"], format!("token-{visit}&special"));
        assert_eq!(form["4Tredir"], "http://test.invalid/destination?x=1&y=2");
    }
    if state.mode == "post_redirect" {
        return (
            StatusCode::TEMPORARY_REDIRECT,
            [("location", "http://example.com/steal")],
        )
            .into_response();
    }
    if state.mode == "reject" {
        return "invalid password".into_response();
    }
    if state.mode != "false_success" {
        state.online.store(true, Ordering::SeqCst);
    }
    if state.campus_form() {
        return "<script>window.location='/keepalive?session-token';</script>".into_response();
    }
    "<a href='/keepalive?secret-token'>Connected</a>".into_response()
}

async fn keepalive(State(state): State<Fixture>, OriginalUri(uri): OriginalUri) -> Response {
    assert_eq!(uri.query(), Some("session-token"));
    state.keepalive_visits.fetch_add(1, Ordering::SeqCst);
    "<title>國立勤益科技大學</title><p>你已成功登入</p><a href='/logout?session-token'>登出</a>"
        .into_response()
}

async fn fixture(mode: &'static str) -> (Engine, Fixture, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = Fixture {
        mode,
        port: addr.port(),
        online: Arc::new(AtomicBool::new(false)),
        visits: Arc::new(AtomicUsize::new(0)),
        submitted: Arc::new(AtomicBool::new(false)),
        keepalive_visits: Arc::new(AtomicUsize::new(0)),
        verification_attempts: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/", post(submit))
        .route("/probe", get(probe))
        .route("/login", get(campus_portal))
        .route("/keepalive", get(keepalive))
        .route("/fgtauth", get(portal))
        .route("/auth", post(submit))
        .with_state(state.clone());
    let origin = format!("http://{addr}");
    let probe = format!("{origin}/probe");
    let portal = if matches!(mode, "direct" | "foreign_redirect") {
        &origin
    } else {
        ""
    };
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        Engine::with_probe_and_portal(&probe, portal).unwrap(),
        state,
        server,
    )
}
#[tokio::test]
async fn redirects_cookies_form_encoding_and_verification() {
    for mode in ["http", "js", "campus_http", "campus_js"] {
        let (engine, state, server) = fixture(mode).await;
        assert_eq!(engine.check().await, Status::NeedsLogin);
        assert_eq!(
            engine
                .connect("s+中文&= 123", "secret&=+\"'\\ 密碼")
                .await
                .unwrap(),
            Status::Online
        );
        assert!(state.submitted.load(Ordering::SeqCst));
        assert_eq!(state.visits.load(Ordering::SeqCst), 2);
        assert_eq!(engine.check().await, Status::Online);
        server.abort();
    }
}

#[tokio::test]
async fn reconnects_through_saved_portal_after_external_probe_stops_working() {
    let (engine, state, server) = fixture("direct").await;
    state.online.store(true, Ordering::SeqCst);
    assert_eq!(
        engine.reconnect(|| panic!("already online")).await,
        Status::Online
    );
    for visit in 1..=2 {
        state.online.store(false, Ordering::SeqCst);
        state.verification_attempts.store(0, Ordering::SeqCst);
        assert_eq!(
            engine
                .reconnect(|| Some(("s+中文&= 123".into(), "secret&=+\"'\\ 密碼".into())))
                .await,
            Status::Online
        );
        assert_eq!(state.visits.load(Ordering::SeqCst), visit);
        assert_eq!(state.keepalive_visits.load(Ordering::SeqCst), visit);
    }
    server.abort();
}

#[tokio::test]
async fn renews_an_online_session_with_the_live_gateway_sequence() {
    let (engine, state, server) = fixture("direct").await;
    state.online.store(true, Ordering::SeqCst);
    assert!(
        engine
            .renew(|| Some(("s+中文&= 123".into(), "secret&=+\"'\\ 密碼".into())))
            .await
    );
    assert_eq!(state.visits.load(Ordering::SeqCst), 1);
    assert_eq!(state.keepalive_visits.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn only_submits_to_configured_portal_origin() {
    let (engine, state, server) = fixture("foreign_redirect").await;
    assert_eq!(
        engine
            .connect("s+中文&= 123", "secret&=+\"'\\ 密碼")
            .await
            .unwrap(),
        Status::Online
    );
    assert!(state.submitted.load(Ordering::SeqCst));
    assert_eq!(state.visits.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn verifies_connectivity_when_gateway_closes_the_post() {
    let (_, state, probe_server) = fixture("direct").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway = format!("http://{}", listener.local_addr().unwrap());
    let probe = format!("http://127.0.0.1:{}/probe", state.port);
    let gateway_state = state.clone();
    let gateway_server = tokio::spawn(async move {
        let (mut connection, _) = listener.accept().await.unwrap();
        let mut line = String::new();
        BufReader::new(&mut connection)
            .read_line(&mut line)
            .await
            .unwrap();
        assert!(line.starts_with("GET /login?"));
        assert!(!line.contains("magic="));
        let body = "<title>國立勤益科技大學</title><form action='/' method='post'>
            <input name='4Tredir' value='http://test.invalid/'>
            <input name='magic' value='0123456789abcdef'>
            <input name='username'><input name='password'></form>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        connection.write_all(response.as_bytes()).await.unwrap();
        connection.shutdown().await.unwrap();

        let (mut connection, _) = listener.accept().await.unwrap();
        line.clear();
        BufReader::new(&mut connection)
            .read_line(&mut line)
            .await
            .unwrap();
        assert!(line.starts_with("POST / "));
        gateway_state.online.store(true, Ordering::SeqCst);
        // Closing without sending headers reproduces the observed transport failure.
    });

    let engine = Engine::with_probe_and_portal(&probe, &gateway).unwrap();
    assert_eq!(
        engine.connect("trial", "fixture").await.unwrap(),
        Status::Online
    );
    gateway_server.await.unwrap();
    probe_server.abort();
}

#[tokio::test]
async fn refuses_foreign_pages_and_bounds_responses() {
    for mode in ["unrelated", "large", "wrong_title", "foreign_form"] {
        let (engine, state, server) = fixture(mode).await;
        assert!(engine
            .connect("s+中文&= 123", "secret&=+\"'\\ 密碼")
            .await
            .is_err());
        assert!(!state.submitted.load(Ordering::SeqCst));
        server.abort();
    }
}
#[tokio::test]
async fn does_not_claim_false_success_or_forward_post() {
    for mode in ["reject", "false_success", "post_redirect"] {
        let (engine, _, server) = fixture(mode).await;
        assert_eq!(
            engine
                .connect("s+中文&= 123", "secret&=+\"'\\ 密碼")
                .await
                .unwrap(),
            Status::LoginFailed
        );
        server.abort();
    }
}
#[tokio::test]
async fn empty_credentials_do_not_submit() {
    let (engine, state, server) = fixture("http").await;
    assert_eq!(
        engine.connect("", "").await.unwrap(),
        Status::MissingCredentials
    );
    assert!(!state.submitted.load(Ordering::SeqCst));
    server.abort();
}
