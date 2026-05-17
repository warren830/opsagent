use axum::{middleware::Next, response::Response};

/// Security headers middleware.
/// Reference: KBP security response headers
pub async fn security_headers(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
    headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());
    // CSP 'default-src self' intentionally omitted — it blocks cross-origin API
    // calls from the frontend. CSP belongs on HTML responses, not JSON API responses.

    response
}
