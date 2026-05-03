//! GCP / GKE integration.
//!
//! Two-tier auth strategy:
//! 1. **Application Default Credentials via `gcloud` CLI** (MVP — what this
//!    file implements). Shells out to `gcloud auth print-access-token`. Good
//!    for local development; requires `gcloud` installed and authenticated.
//! 2. **Service Account JSON key via `yup-oauth2`** (future — tracked as
//!    TODO in `get_gke_access_token_for_account`). Would read
//!    `cloud_accounts.config.service_account_json` and mint tokens without
//!    shelling out. This is the production path.
//!
//! Tokens are short-lived (~1 hour). Callers should request a fresh one for
//! each operation rather than caching.

use crate::error::{AppError, AppResult};
use crate::models::cloud_account::CloudAccount;

/// Obtain a short-lived OAuth2 access token for Google Cloud APIs.
///
/// Scope is whatever `gcloud` is currently configured with (typically
/// `https://www.googleapis.com/auth/cloud-platform`, which covers GKE).
pub async fn get_gke_access_token() -> AppResult<String> {
    let output = tokio::process::Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .output()
        .await
        .map_err(|e| AppError::Kubernetes(format!("gcloud CLI not available: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Kubernetes(format!(
            "gcloud print-access-token failed: {}",
            stderr.trim()
        )));
    }

    let token = String::from_utf8(output.stdout)
        .map_err(|e| AppError::Kubernetes(format!("token not UTF-8: {e}")))?
        .trim()
        .to_string();

    if token.is_empty() {
        return Err(AppError::Kubernetes("gcloud returned empty token".to_string()));
    }

    Ok(token)
}

/// Future-friendly signature: accept CloudAccount so we can later switch to
/// service-account-JSON auth per account. For now, always falls through to
/// ADC via `gcloud`.
///
/// TODO: if `account.config.service_account_json` is present, use
/// `yup-oauth2::ServiceAccountAuthenticator` to mint a token without shelling
/// out. This removes the gcloud-CLI dependency for production deployments.
#[allow(dead_code)]
pub async fn get_gke_access_token_for_account(_account: Option<&CloudAccount>) -> AppResult<String> {
    get_gke_access_token().await
}
