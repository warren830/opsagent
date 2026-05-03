use crate::error::{AppError, AppResult};
use crate::models::cloud_account::CloudAccount;
use crate::models::cluster::Cluster;

/// Parse PEM-encoded certificate bytes into DER bytes for kube::Config root_cert.
fn rustls_pem_to_der(pem_bytes: &[u8]) -> AppResult<Vec<u8>> {
    let pem_str =
        std::str::from_utf8(pem_bytes).map_err(|e| AppError::Kubernetes(format!("CA cert not valid UTF-8: {e}")))?;
    // Strip PEM header/footer and decode the inner base64
    let b64: String = pem_str.lines().filter(|l| !l.starts_with("-----")).collect();
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| AppError::Kubernetes(format!("CA cert inner base64 decode failed: {e}")))
}

/// Build env vars for AWS CLI: profile → assume-role → root profile fallback.
/// Extracted from cluster.rs for reuse across cluster discovery, screener, and K8s client.
pub async fn build_account_env(
    account: &CloudAccount,
    root_profile: &Option<String>,
) -> Result<Vec<(String, String)>, String> {
    // Account has its own profile — use it directly
    if let Some(ref profile) = account.profile {
        return Ok(vec![("AWS_PROFILE".to_string(), profile.clone())]);
    }

    // Account has role_arn — assume-role using root profile
    if let Some(ref role_arn) = account.role_arn {
        let mut cmd = tokio::process::Command::new("aws");
        cmd.args([
            "sts",
            "assume-role",
            "--role-arn",
            role_arn,
            "--role-session-name",
            "ops-k8s",
            "--duration-seconds",
            "900",
            "--output",
            "json",
        ]);
        if let Some(profile) = root_profile {
            cmd.args(["--profile", profile]);
        }
        let output = cmd.output().await.map_err(|e| format!("aws CLI error: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("assume-role failed: {}", stderr.trim()));
        }
        let body: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|e| format!("parse error: {e}"))?;
        let creds = body.pointer("/Credentials").ok_or("No Credentials")?;
        let ak = creds
            .get("AccessKeyId")
            .and_then(|v| v.as_str())
            .ok_or("Missing AccessKeyId")?;
        let sk = creds
            .get("SecretAccessKey")
            .and_then(|v| v.as_str())
            .ok_or("Missing SecretAccessKey")?;
        let st = creds
            .get("SessionToken")
            .and_then(|v| v.as_str())
            .ok_or("Missing SessionToken")?;
        return Ok(vec![
            ("AWS_ACCESS_KEY_ID".to_string(), ak.to_string()),
            ("AWS_SECRET_ACCESS_KEY".to_string(), sk.to_string()),
            ("AWS_SESSION_TOKEN".to_string(), st.to_string()),
        ]);
    }

    // No credentials — fall back to root profile if available
    match root_profile {
        Some(p) => Ok(vec![("AWS_PROFILE".to_string(), p.clone())]),
        None => Ok(vec![]),
    }
}

/// Build a kube::Client for a cluster, dispatching on cloud provider.
///
/// - AWS EKS: `aws eks get-token` via assume-role
/// - GCP GKE: `gcloud auth print-access-token` (ADC)
/// - Future (Alicloud ACK, Azure AKS): add branches here
pub async fn build_k8s_client(pool: &sqlx::PgPool, cluster: &Cluster) -> AppResult<kube::Client> {
    match cluster.cloud.as_str() {
        "gcp" => build_gke_client(cluster).await,
        _ => build_eks_client(pool, cluster).await,
    }
}

/// Build a kube::Client for an EKS cluster.
///
/// 1. Extract endpoint + certificate_authority from cluster.config
/// 2. Resolve AWS credentials for the cluster's account
/// 3. Call `aws eks get-token` to get a Bearer token
/// 4. Build kube::Config → kube::Client
pub async fn build_eks_client(pool: &sqlx::PgPool, cluster: &Cluster) -> AppResult<kube::Client> {
    let endpoint = cluster.config.get("endpoint").and_then(|v| v.as_str()).ok_or_else(|| {
        AppError::BadRequest("Cluster config missing 'endpoint'. Re-run cluster discovery.".to_string())
    })?;

    let ca_data = cluster
        .config
        .get("certificate_authority")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::BadRequest(
                "Cluster config missing 'certificate_authority'. Re-run cluster discovery.".to_string(),
            )
        })?;

    // Resolve AWS credentials for the cluster's account
    let account = if let Some(ref acct_id) = cluster.account_id {
        sqlx::query_as::<_, CloudAccount>(
            "SELECT * FROM cloud_accounts WHERE account_id = $1 AND provider = 'aws' ORDER BY CASE WHEN source = 'manual' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(acct_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Internal(format!("DB error: {e}")))?
    } else {
        None
    };

    let root_profile: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT profile FROM cloud_accounts WHERE provider = 'aws' AND profile IS NOT NULL LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let env_vars = if let Some(ref acct) = account {
        build_account_env(acct, &root_profile)
            .await
            .map_err(|e| AppError::Internal(format!("AWS auth: {e}")))?
    } else {
        vec![]
    };

    // Get EKS bearer token
    let region = cluster.region.as_deref().unwrap_or("us-east-1");
    let mut cmd = tokio::process::Command::new("aws");
    cmd.args([
        "eks",
        "get-token",
        "--cluster-name",
        &cluster.name,
        "--region",
        region,
        "--output",
        "json",
    ]);
    for (k, v) in &env_vars {
        cmd.env(k, v);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Kubernetes(format!("aws eks get-token failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Kubernetes(format!(
            "aws eks get-token error: {}",
            stderr.trim()
        )));
    }

    let token_body: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| AppError::Kubernetes(format!("parse token: {e}")))?;

    let token = token_body
        .pointer("/status/token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Kubernetes("No token in get-token response".to_string()))?;

    // Decode CA certificate (base64)
    use base64::Engine;
    let ca_bytes = base64::engine::general_purpose::STANDARD
        .decode(ca_data)
        .map_err(|e| AppError::Kubernetes(format!("Invalid CA base64: {e}")))?;

    // Build kube::Config from parts
    // ca_bytes is PEM (base64 decoded from DB). kube wants DER in root_cert.
    let ca_der = rustls_pem_to_der(&ca_bytes)?;
    tracing::debug!(
        "Building kube client for {} (endpoint={}, ca_der_len={})",
        cluster.name,
        endpoint,
        ca_der.len()
    );

    let cluster_url: http::Uri = endpoint
        .parse()
        .map_err(|e| AppError::Kubernetes(format!("Invalid endpoint URL: {e}")))?;

    let kube_config = kube::Config {
        cluster_url,
        default_namespace: "default".to_string(),
        root_cert: Some(vec![ca_der]),
        connect_timeout: Some(std::time::Duration::from_secs(30)),
        read_timeout: Some(std::time::Duration::from_secs(60)),
        write_timeout: Some(std::time::Duration::from_secs(60)),
        accept_invalid_certs: false,
        auth_info: kube::config::AuthInfo {
            token: Some(secrecy::SecretString::from(token.to_string())),
            ..Default::default()
        },
        proxy_url: None,
        tls_server_name: None,
        disable_compression: false,
        headers: vec![],
    };

    let client = kube::Client::try_from(kube_config)
        .map_err(|e| AppError::Kubernetes(format!("Failed to build K8s client: {e}")))?;

    tracing::debug!("Kube client built successfully for {}", cluster.name);

    Ok(client)
}

/// Build a kube::Client for a GKE cluster.
///
/// 1. Extract endpoint + CA cert from cluster.config (accepts either `ca_cert`
///    from manual POST or `certificate_authority` from future discovery)
/// 2. Get a short-lived OAuth token via `gcloud auth print-access-token`
///    (MVP — production should switch to service-account JSON via yup-oauth2)
/// 3. Build kube::Config → kube::Client
///
/// GKE's `masterAuth.clusterCaCertificate` is base64-encoded PEM, same shape
/// as EKS's CA field, so we reuse `rustls_pem_to_der`.
pub async fn build_gke_client(cluster: &Cluster) -> AppResult<kube::Client> {
    let endpoint = cluster
        .config
        .get("endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("GKE config missing 'endpoint'".to_string()))?;

    // Accept either key — 'ca_cert' is what manual POST uses, 'certificate_authority'
    // matches EKS naming for future uniform discovery.
    let ca_data = cluster
        .config
        .get("ca_cert")
        .or_else(|| cluster.config.get("certificate_authority"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::BadRequest(
                "GKE config missing 'ca_cert' (or 'certificate_authority')".to_string(),
            )
        })?;

    // Mint a fresh OAuth token. Valid for ~1h; we don't cache across calls —
    // rollout_watcher runs at 60s interval, so worst case is one extra gcloud
    // invocation per minute.
    let token = crate::services::gcp::get_gke_access_token().await?;

    // CA: base64 → PEM bytes → DER
    use base64::Engine;
    let ca_bytes = base64::engine::general_purpose::STANDARD
        .decode(ca_data)
        .map_err(|e| AppError::Kubernetes(format!("Invalid GKE CA base64: {e}")))?;
    let ca_der = rustls_pem_to_der(&ca_bytes)?;

    tracing::debug!(
        "Building GKE client for {} (endpoint={}, ca_der_len={})",
        cluster.name,
        endpoint,
        ca_der.len()
    );

    let cluster_url: http::Uri = endpoint
        .parse()
        .map_err(|e| AppError::Kubernetes(format!("Invalid GKE endpoint URL: {e}")))?;

    let kube_config = kube::Config {
        cluster_url,
        default_namespace: "default".to_string(),
        root_cert: Some(vec![ca_der]),
        connect_timeout: Some(std::time::Duration::from_secs(30)),
        read_timeout: Some(std::time::Duration::from_secs(60)),
        write_timeout: Some(std::time::Duration::from_secs(60)),
        accept_invalid_certs: false,
        auth_info: kube::config::AuthInfo {
            token: Some(secrecy::SecretString::from(token)),
            ..Default::default()
        },
        proxy_url: None,
        tls_server_name: None,
        disable_compression: false,
        headers: vec![],
    };

    let client = kube::Client::try_from(kube_config)
        .map_err(|e| AppError::Kubernetes(format!("Failed to build GKE client: {e}")))?;

    tracing::debug!("GKE client built successfully for {}", cluster.name);

    Ok(client)
}

/// Load a cluster from DB and verify the caller has access.
pub async fn load_and_authorize_cluster(
    pool: &sqlx::PgPool,
    cluster_id: uuid::Uuid,
    auth_user: &crate::middleware::auth::AuthUser,
) -> AppResult<Cluster> {
    let cluster = sqlx::query_as::<_, Cluster>("SELECT * FROM clusters WHERE id = $1")
        .bind(cluster_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Cluster not found".to_string()))?;

    if !auth_user.is_super_admin() && cluster.tenant_id != auth_user.tenant_id {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }

    Ok(cluster)
}
