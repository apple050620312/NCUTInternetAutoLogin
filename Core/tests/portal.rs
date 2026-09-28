use axum::{
    extract::{Form, State},
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

#[derive(Clone)]
struct Fixture {
    mode: &'static str,
    online: Arc<AtomicBool>,
    visits: Arc<AtomicUsize>,
    submitted: Arc<AtomicBool>,
}
async fn probe(State(state): State<Fixture>) -> Response {
    if state.online.load(Ordering::SeqCst) {
        return StatusCode::NO_CONTENT.into_response();
    }
    match state.mode {
        "unrelated" => "<title>Other network</title>".into_response(),
        "large" => "x".repeat(300_000).into_response(),
        "js" => "<script>window.location = '/fgtauth?from-url';</script>".into_response(),
        _ => Redirect::temporary("/fgtauth?from-url").into_response(),
    }
}
async fn portal(State(state): State<Fixture>) -> Response {
    state.visits.fetch_add(1, Ordering::SeqCst);
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
    ([("set-cookie", "session=fixture; Path=/; HttpOnly")], format!("<title>{title}</title><form method='post' action='{action}'><input name='username'><input type='password' name='password'><input name='magic' value='token&amp;special'></form>")).into_response()
}
async fn submit(
    State(state): State<Fixture>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    state.submitted.store(true, Ordering::SeqCst);
    assert!(headers
        .get("cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("session=fixture"));
    assert_eq!(form["username"], "s+中文&= 123");
    assert_eq!(form["password"], "secret&=+\"'\\ 密碼");
    assert_eq!(form["magic"], "token&special");
    assert!(state.visits.load(Ordering::SeqCst) >= 2 || state.mode == "js");
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
    "<a href='/keepalive?secret-token'>Connected</a>".into_response()
}
async fn fixture(mode: &'static str) -> (Engine, Fixture, tokio::task::JoinHandle<()>) {
    let state = Fixture {
        mode,
        online: Arc::new(AtomicBool::new(false)),
        visits: Arc::new(AtomicUsize::new(0)),
        submitted: Arc::new(AtomicBool::new(false)),
    };
    let app = Router::new()
        .route("/probe", get(probe))
        .route("/fgtauth", get(portal))
        .route("/auth", post(submit))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/probe", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (Engine::with_probe(&url).unwrap(), state, server)
}
#[tokio::test]
async fn redirects_cookies_form_encoding_and_verification() {
    for mode in ["http", "js"] {
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
        assert_eq!(engine.check().await, Status::Online);
        server.abort();
    }
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
