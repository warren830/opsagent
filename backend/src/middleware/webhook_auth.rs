//! Webhook authentication helper.
//!
//! Alerting providers (Grafana / Datadog / Dynatrace) push to unauthenticated
//! endpoints from outside our VPC. We still need to know *which tenant* a
//! given alert belongs to, so the operator provisions a per-tenant shared
//! token (stored as bcrypt hash in `webhook_secrets`) and the provider sends
//! it back on every call in one of:
//!
//! - Grafana: `X-Webhook-Token: <token>`
//! - Datadog: `X-Webhook-Signature: <token>` (MVP: treated as a shared token,
//!   not an HMAC signature — future work: HMAC-SHA256 over the body)
//! - Dynatrace: `Authorization: Bearer <token>`
//!
//! On a successful match the caller receives the matching `tenant_id` so the
//! resulting `issues` row can be written with the correct tenant scope.

use bcrypt::verify;
use sqlx::PgPool;
use uuid::Uuid;

/// Verify a plaintext webhook token against `webhook_secrets` rows for the
/// given provider. Returns the matching `tenant_id` on success, `None` on
/// mismatch. Short-circuits on the first hit so the happy path is fast; the
/// bcrypt comparison is constant-time per row.
///
/// `provider` must be one of `grafana` / `datadog` / `dynatrace` (enforced by
/// the CHECK constraint on the table). An empty `token` always returns
/// `None` — we never treat a blank string as a match.
pub async fn verify_webhook_secret(
    pool: &PgPool,
    provider: &str,
    token: &str,
) -> Option<Uuid> {
    if token.is_empty() {
        return None;
    }

    let rows: Vec<(Uuid, String)> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT tenant_id, secret_hash
           FROM webhook_secrets
           WHERE provider = $1 AND enabled = TRUE",
    )
    .bind(provider)
    .fetch_all(pool)
    .await
    .ok()?;

    for (tenant_id, hash) in rows {
        if let Ok(true) = verify(token, &hash) {
            return Some(tenant_id);
        }
    }
    None
}
