//! Serves the public pages as Markdown to AI crawlers and to anything that asks for it.

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::app::AppState;
use crate::crawlers;
use crate::html_md::convert;

/// Paths that are never public pages.
const PRIVATE: &[&str] = &[
    "/app",
    "/api",
    "/mcp",
    "/oauth",
    "/share",
    "/r/",
    "/l/",
    "/admin",
    "/auth",
    "/login",
    "/invite",
    "/logout",
    "/.well-known",
];

fn public(path: &str) -> bool {
    !PRIVATE.iter().any(|p| path.starts_with(p))
}

/// Whether the request asks for Markdown: `Accept: text/markdown`, or an AI crawler's user agent.
fn wants(headers: &HeaderMap) -> bool {
    let value = |name: header::HeaderName| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    value(header::ACCEPT).contains("text/markdown")
        || crawlers::detect(value(header::USER_AGENT)).is_some()
}

/// Answers a public HTML page with its Markdown when the request wants it; a `.md` suffix always does.
pub async fn negotiate(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let get = matches!(*req.method(), Method::GET | Method::HEAD);
    let path = req.uri().path().to_string();
    if !state.config.marketing || !get || !public(&path) {
        return next.run(req).await;
    }
    let mut asked = wants(req.headers());
    if let Some(page) = path.strip_suffix(".md") {
        let query = req
            .uri()
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default();
        let mut parts = req.uri().clone().into_parts();
        parts.path_and_query = format!("{}{query}", if page.is_empty() { "/" } else { page })
            .parse()
            .ok();
        if let Ok(uri) = axum::http::Uri::from_parts(parts) {
            *req.uri_mut() = uri;
            asked = true;
        }
    }
    let res = next.run(req).await;
    let html = res.status().is_success()
        && res
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes().starts_with(b"text/html"));
    if !html {
        return res;
    }
    let vary = HeaderValue::from_static("Accept, User-Agent");
    let (mut parts, body) = res.into_parts();
    parts.headers.insert(header::VARY, vary);
    if !asked {
        return Response::from_parts(parts, body);
    }
    let Ok(bytes) = axum::body::to_bytes(body, 8 << 20).await else {
        return Response::from_parts(parts, axum::body::Body::empty());
    };
    let text = convert(&String::from_utf8_lossy(&bytes));
    parts.headers.remove(header::CONTENT_LENGTH);
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    Response::from_parts(parts, axum::body::Body::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_by_header_or_crawler() {
        let mut h = HeaderMap::new();
        assert!(!wants(&h));
        h.insert(
            header::ACCEPT,
            HeaderValue::from_static("text/markdown, text/html"),
        );
        assert!(wants(&h));
        let mut h = HeaderMap::new();
        h.insert(
            header::USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (compatible; GPTBot/1.2)"),
        );
        assert!(wants(&h));
        assert!(public("/pricing") && !public("/app/x.com") && !public("/mcp"));
    }
}
