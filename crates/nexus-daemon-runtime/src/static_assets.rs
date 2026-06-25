//! Embedded Web UI static assets — release-mode serving via `rust-embed`.
//!
//! In release builds, `apps/web/dist` is embedded into the `nexus42` binary at
//! compile time.  The daemon serves these assets at the server root (`/`) as an
//! unauthenticated SPA shell.  All data routes remain behind the loopback
//! `/v1/local/*` API under the existing V1.20 keyless-localhost model.
//!
//! # SPA fallback
//!
//! Unmatched `GET` / `HEAD` paths (non-`/v1/local/*`) are served `index.html`
//! so the React Router can handle client-side routing.  Non-GET requests and
//! paths that don't match any embedded file return `404 NOT FOUND`.
//!
//! # Cache headers
//!
//! - `/assets/*` (hashed Vite output)   → `Cache-Control: public, max-age=31536000, immutable`
//! - `index.html` (SPA entry point)     → `Cache-Control: no-cache`
//!
//! # Build cfg
//!
//! This module and its router fallback are **release-only**
//! (`#[cfg(not(debug_assertions))]` on the `mod` declaration in `lib.rs` and
//! the `.fallback(...)` registration in `api/mod.rs`). In debug/test builds
//! the SPA fallback is not wired, so unmatched paths return the framework 404
//! (avoids masking API routing bugs in tests). Dev uses the Vite dev server,
//! which proxies `/v1/local/*` to the daemon; the browser loads from the Vite
//! port, never the daemon's `/`. Release binaries embed the real Vite dist
//! (built via `pnpm --filter web build`); `build.rs` creates a stub dist when
//! absent so this crate compiles even without a prior web build.

use axum::{
    body::Body,
    http::{header, HeaderMap, Method, StatusCode, Uri},
    response::IntoResponse,
};
use rust_embed::RustEmbed;

/// Embedded `apps/web/dist` — populated at `cargo build` time.
///
/// The macro reads `../../apps/web/dist` relative to this crate's `Cargo.toml`.
/// When the dist changes, `rust-embed`'s build-script tracking triggers a
/// rebuild automatically.
#[derive(RustEmbed)]
#[folder = "../../apps/web/dist"]
pub struct WebAssets;

/// Axum handler: serve the embedded SPA static assets at server root `/`.
///
/// # Behaviour
///
/// | Request                      | Response                          |
/// |------------------------------|-----------------------------------|
/// | `GET /`                      | `200 index.html`                  |
/// | `GET /assets/<hash>.js`      | `200 <file>` (cached 1 year)      |
/// | `GET /works` (client route)  | `200 index.html` (SPA fallback)   |
/// | `POST /works`                | `404 Not Found`                   |
/// | `GET /v1/local/...`          | *handled by existing API routes*  |
///
/// The function is designed as a **fallback** inside the axum router — it only
/// receives requests that were NOT matched by any `/v1/local/*` or other
/// explicit route.
///
/// # Panics
///
/// Panics if a hard-coded header value (cache-control, content-type) fails to
/// parse — which should never happen in practice because all values are
/// compile-time constants with known-valid formats.
#[must_use]
pub async fn serve_embedded_app(method: Method, uri: Uri) -> impl IntoResponse {
    // Only serve GET and HEAD for static assets.
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match WebAssets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mut headers = HeaderMap::new();

            // Hashed Vite assets get a one-year immutable cache.
            if path.starts_with("assets/") {
                headers.insert(
                    header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable"
                        .parse()
                        .expect("valid header value"),
                );
            } else {
                // The entry point and any legacy non-hashed files get no-cache.
                headers.insert(
                    header::CACHE_CONTROL,
                    "no-cache".parse().expect("valid header value"),
                );
            }

            headers.insert(
                header::CONTENT_TYPE,
                mime.as_ref().parse().expect("valid MIME"),
            );

            (StatusCode::OK, headers, Body::from(file.data)).into_response()
        }
        None => {
            // SPA fallback: any unmatched GET path that is not an API route
            // gets `index.html` so the React Router can take over.
            if let Some(index_file) = WebAssets::get("index.html") {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CACHE_CONTROL,
                    "no-cache".parse().expect("valid header value"),
                );
                headers.insert(
                    header::CONTENT_TYPE,
                    "text/html".parse().expect("valid MIME"),
                );
                (StatusCode::OK, headers, Body::from(index_file.data)).into_response()
            } else {
                // No index.html embedded at all — the build is missing the dist.
                (StatusCode::NOT_FOUND, "SPA not available").into_response()
            }
        }
    }
}
