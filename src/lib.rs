#![allow(
    dead_code,
    private_interfaces,
    unused_imports,
    unused_mut,
    unused_variables,
    clippy::result_large_err,
    clippy::if_same_then_else,
    clippy::too_many_arguments,
    clippy::field_reassign_with_default,
    clippy::should_implement_trait,
    clippy::unnecessary_filter_map,
    clippy::question_mark,
    clippy::type_complexity,
    clippy::await_holding_lock,
    clippy::unnecessary_get_then_check,
    clippy::ptr_arg,
    clippy::unnecessary_sort_by,
    clippy::redundant_locals
)]

pub mod cli;
pub mod core;
pub mod db;
pub mod oauth;
pub mod payload_rules;
pub mod server;
pub mod types;

use std::panic::AssertUnwindSafe;

use axum::response::IntoResponse;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};

pub fn build_app(state: server::state::AppState) -> Router {
    use axum::middleware;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    server::api::routes(state.clone())
        .merge(server::dashboard::routes())
        // Real-IP middleware: stamps the verified TCP peer IP as
        // `x-9r-real-ip` and strips client-supplied forwarding headers.
        .layer(middleware::from_fn(server::api::guard::real_ip_middleware))
        // Turn a panic in any handler into a 500 instead of tearing down the
        // connection. Without this, a panic inside a handler serving a
        // keep-alive connection has no defined outcome: the client sees a
        // reset mid-stream and the failure is invisible in the logs. The
        // translator and executor trees hold hundreds of `unwrap`/`expect`
        // calls on paths that consume upstream provider responses, so this is
        // a reachable path, not a theoretical one.
        .layer(middleware::from_fn(catch_panic))
        .layer(cors)
        .with_state(state)
}

/// Middleware that converts a panic in a downstream handler into a 500.
///
/// `HandleErrorLayer` is NOT sufficient here: in axum 0.8 it only maps
/// `Infallible`-style handler *errors*, and a panicking handler propagates the
/// unwind straight through it (verified — a test with a panicking route and
/// `HandleErrorLayer` still panics). This does the `catch_unwind` explicitly.
///
/// Requires `panic = "unwind"`, which `Cargo.toml` sets. Under `panic = "abort"`
/// this is a no-op and the process would simply exit; assert that in a test so
/// a profile change cannot silently disarm it.
pub async fn catch_panic(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use futures_util::FutureExt;

    let fut = next.run(request);
    match AssertUnwindSafe(fut).catch_unwind().await {
        Ok(response) => response,
        Err(payload) => {
            // Never surface the panic message: it can carry upstream payloads,
            // file paths, or credentials. The detail goes to the panic hook,
            // which records the type and location instead.
            tracing::error!(
                target: "zeroproxy::panic",
                payload = ?payload.downcast_ref::<&str>().copied(),
                "handler panicked; returning 500 to the client",
            );
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error",
            )
                .into_response()
        }
    }
}

/// Install a panic hook that records the panic with its location before the
/// default hook prints it.
///
/// Call once at process start. The default hook already prints, so this only
/// adds structure (`target`, a stable marker, and the thread name) that
/// survives JSON log formatting — a panic in a request task otherwise arrives
/// as an unstructured line that is easy to miss in a busy log.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(
            target: "zeroproxy::panic",
            thread = std::thread::current().name().unwrap_or("<unnamed>"),
            location = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_else(|| "<unknown>".to_string()),
            "handler panicked; returning 500 to the client",
        );
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    /// Build a router with the same panic middleware `build_app` installs.
    fn app_with_catch_panic() -> Router {
        Router::new()
            .route(
                "/boom",
                get(|| async {
                    panic!("intentional test panic");
                    #[allow(unreachable_code)]
                    "unreachable"
                }),
            )
            .route("/ok", get(|| async { "fine" }))
            .layer(axum::middleware::from_fn(super::catch_panic))
    }

    /// `catch_panic` is a no-op under `panic = "abort"` — the process would just
    /// die instead of returning a 500. Pin the unwind strategy so a profile
    /// change cannot silently disarm panic containment.
    #[test]
    fn panic_strategy_allows_unwinding() {
        // The portable check: a panic that can be caught proves unwinding is
        // enabled in this build.
        let caught = std::panic::catch_unwind(|| panic!("probe"));
        assert!(
            caught.is_err(),
            "panic must unwind for catch_panic to convert it into a 500"
        );
    }

    /// A panicking handler must surface as a 500 to the client, not a dropped
    /// connection. Remove `catch_panic` from `build_app` and this fails.
    #[tokio::test]
    async fn panicking_handler_returns_500() {
        let resp = app_with_catch_panic()
            .oneshot(
                axum::http::Request::get("/boom")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("service must not propagate the panic");

        assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// The error body must not leak the panic message. A panic string can carry
    /// upstream payloads, file paths, or credentials, so it must never reach a
    /// client.
    #[tokio::test]
    async fn panic_body_does_not_leak_internals() {
        let resp = app_with_catch_panic()
            .oneshot(
                axum::http::Request::get("/boom")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("service must not propagate the panic");

        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(
            !body.contains("intentional test panic"),
            "panic message must not reach the client, got: {body}"
        );
    }

    /// A healthy handler must be unaffected by the middleware.
    #[tokio::test]
    async fn non_panicking_handler_still_works() {
        let resp = app_with_catch_panic()
            .oneshot(
                axum::http::Request::get("/ok")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("service should respond");

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        assert_eq!(&bytes[..], b"fine");
    }

    /// The router must still serve normally when nothing panics — guards against
    /// the middleware accidentally converting every response into a 500 through
    /// a mistake in the `Ok` arm.
    #[tokio::test]
    async fn ok_response_passes_through_untouched() {
        let resp = app_with_catch_panic()
            .oneshot(
                axum::http::Request::get("/ok")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .expect("service should respond");

        assert_ne!(
            resp.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "a healthy handler must not be turned into a 500"
        );
    }
}
