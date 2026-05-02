use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post, put},
};
use std::net::SocketAddr;
use tokio::signal;
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};

use ops::{AppState, config::AppConfig, db, handlers, middleware, services};

#[tokio::main]
async fn main() {
    // Load .env file (ignore errors if not present)
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ops=debug,tower_http=debug".into()),
        )
        .init();

    // Load config
    let config = AppConfig::from_env();
    tracing::info!("Starting Ops backend (env={:?})", config.env);

    // Create database pool
    let pool = db::create_pool(&config).await.expect("Failed to create database pool");

    // Run migrations
    db::run_migrations(&pool)
        .await
        .expect("Failed to run database migrations");

    // Seed default admin user if no users exist
    seed_admin_user(&pool).await;

    // Build app state
    let state = AppState {
        pool,
        config: config.clone(),
        rca_registry: std::sync::Arc::new(services::rca::RcaRegistry::new()),
        timeline_bus: std::sync::Arc::new(services::incident::timeline_bus::TimelineBus::new()),
    };

    // Spawn token cleanup task (every 6 hours)
    {
        let cleanup_pool = state.pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                if let Err(e) = services::refresh_token::cleanup_expired(&cleanup_pool).await {
                    tracing::error!("Refresh token cleanup failed: {}", e);
                }
                if let Err(e) = services::oauth_state::cleanup_expired_states(&cleanup_pool).await {
                    tracing::error!("OAuth state cleanup failed: {}", e);
                }
                tracing::debug!("Token/state cleanup completed");
            }
        });
    }

    // Spawn prediction scheduler (background task)
    if std::env::var("SKIP_PREDICTION").unwrap_or_default() != "true" {
        let scheduler_pool = state.pool.clone();
        let interval_secs: u64 = std::env::var("PREDICTION_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1800);
        tracing::info!("Prediction scheduler enabled (interval={}s)", interval_secs);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            // Skip the first immediate tick — let the server warm up
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(e) = services::prediction::run_prediction_check(&scheduler_pool).await {
                    tracing::error!("Prediction check failed: {}", e);
                }
            }
        });
    } else {
        tracing::info!("Prediction scheduler disabled (SKIP_PREDICTION=true)");
    }

    // Spawn Organization account sync task
    {
        let sync_pool = state.pool.clone();
        let interval_secs: u64 = std::env::var("ORG_SYNC_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(6 * 3600); // default 6 hours
        tracing::info!("Organization sync scheduler enabled (interval={}s)", interval_secs);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                match services::cloud_account::sync_org_accounts(&sync_pool, None).await {
                    Ok(r) => {
                        tracing::info!(
                            "Org sync completed: added={}, updated={}, removed={}",
                            r.added,
                            r.updated,
                            r.removed
                        );
                    }
                    Err(e) => {
                        tracing::error!("Org sync failed: {}", e);
                    }
                }
            }
        });
    }

    // Spawn cluster discovery scheduler (background task)
    {
        let discover_pool = state.pool.clone();
        let interval_secs: u64 = std::env::var("CLUSTER_DISCOVER_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(6 * 3600); // default 6 hours
        tracing::info!("Cluster discovery scheduler enabled (interval={}s)", interval_secs);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                match services::cluster::discover_all_clusters(&discover_pool, None).await {
                    Ok(r) => {
                        tracing::info!(
                            "Cluster discovery completed: discovered={}, errors={}",
                            r.discovered,
                            r.errors.len()
                        );
                        for err in &r.errors {
                            tracing::warn!("Cluster discovery error: {}", err);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Cluster discovery failed: {}", e);
                    }
                }
            }
        });
    }

    // Spawn cron-based job scheduler (evaluates every 60s)
    {
        let scheduler_pool = state.pool.clone();
        tracing::info!("Job scheduler started (evaluating every 60s)");
        tokio::spawn(services::scheduler::run_scheduler(scheduler_pool));
    }

    // Spawn rollout status watcher (polls Argo Rollout CRDs for phase/step changes)
    {
        let watcher_pool = state.pool.clone();
        tokio::spawn(services::rollout_watcher::run_rollout_watcher(watcher_pool));
    }

    // Spawn SLO snapshot scheduler — captures one `error_budget_snapshots`
    // row per enabled SLO every `SLO_SNAPSHOT_INTERVAL_SECS` (default 300s).
    if std::env::var("SKIP_SLO_SNAPSHOT").unwrap_or_default() != "true" {
        let snapshot_pool = state.pool.clone();
        tokio::spawn(services::slo::snapshot_runner::run_snapshot_loop(
            snapshot_pool,
        ));
    } else {
        tracing::info!("SLO snapshot scheduler disabled (SKIP_SLO_SNAPSHOT=true)");
    }

    // Build CORS layer
    let cors = middleware::cors::build_cors_layer(&config);

    // Build router
    let app = build_router(state)
        .layer(cors)
        .layer(axum_middleware::from_fn(middleware::security::security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(20 * 1024 * 1024)); // 20MB (images can be large)

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.backend_port));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("Server error");

    tracing::info!("Server shut down gracefully");
}

fn build_router(state: AppState) -> Router {
    let jwt_secret = state.config.jwt_secret.clone();

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(handlers::health::health_check))
        .route("/api/auth/login", post(handlers::auth::login))
        // OAuth routes (public)
        .route("/api/auth/providers", get(handlers::oauth::providers))
        .route("/api/auth/microsoft/login", get(handlers::oauth::microsoft_login))
        .route(
            "/api/auth/microsoft/callback",
            post(handlers::oauth::microsoft_callback),
        )
        .route("/api/auth/cognito/login", get(handlers::oauth::cognito_login))
        .route("/api/auth/cognito/callback", post(handlers::oauth::cognito_callback))
        .route("/api/auth/refresh", post(handlers::oauth::refresh))
        .route("/api/auth/revoke", post(handlers::oauth::revoke))
        // Invite redemption (public — user has no account yet)
        .route("/api/auth/invite/{token}", get(handlers::user::validate_invite))
        .route("/api/auth/invite/{token}/redeem", post(handlers::user::redeem_invite))
        // Alerting webhooks (no auth — external services cannot send JWT)
        .route("/api/alerts", post(handlers::alerts::receive))
        .route("/api/alerts/datadog", post(handlers::alerts::receive_datadog))
        .route("/api/alerts/dynatrace", post(handlers::alerts::receive_dynatrace))
        // ArgoCD notification webhook (no auth — cluster-internal)
        .route("/api/webhooks/argocd", post(handlers::argocd_webhook::receive));

    // Protected routes (auth required)
    let protected_routes = Router::new()
        .route("/api/auth/logout", post(handlers::auth::logout))
        .route("/api/auth/me", get(handlers::auth::me))
        .route("/api/auth/change-password", put(handlers::auth::change_password))
        .route("/api/auth/revoke-all", post(handlers::oauth::revoke_all))
        // Tenants
        .route("/api/tenants", get(handlers::tenant::list_tenants))
        .route("/api/tenants", post(handlers::tenant::create_tenant))
        .route("/api/tenants/{id}", get(handlers::tenant::get_tenant))
        .route("/api/tenants/{id}", put(handlers::tenant::update_tenant))
        .route("/api/tenants/{id}", delete(handlers::tenant::delete_tenant))
        // Users
        .route("/api/users", get(handlers::user::list_users))
        .route("/api/users", post(handlers::user::create_user))
        .route("/api/users/invite", post(handlers::user::invite_user))
        .route("/api/users/{id}", put(handlers::user::update_user))
        .route("/api/users/{id}", delete(handlers::user::delete_user))
        .route("/api/users/{id}/resend-invite", post(handlers::user::resend_invite))
        // Glossary
        .route(
            "/api/glossary",
            get(handlers::glossary::list).post(handlers::glossary::create),
        )
        .route(
            "/api/glossary/{id}",
            put(handlers::glossary::update).delete(handlers::glossary::delete),
        )
        // Skills (DB + git clone)
        .route("/api/skills", get(handlers::skill::list).post(handlers::skill::create))
        .route("/api/skills/discover", post(handlers::skill::discover))
        .route("/api/skills/inline", post(handlers::skill::create_inline))
        .route("/api/skills/{id}/inline", put(handlers::skill::update_inline))
        .route(
            "/api/skills/{id}",
            put(handlers::skill::update).delete(handlers::skill::delete),
        )
        // Cloud Accounts
        .route(
            "/api/accounts",
            get(handlers::cloud_account::list).post(handlers::cloud_account::create),
        )
        .route(
            "/api/accounts/{id}",
            put(handlers::cloud_account::update).delete(handlers::cloud_account::delete),
        )
        // Account Access Control
        .route(
            "/api/accounts/{id}/users",
            get(handlers::account_access::list_account_users),
        )
        .route("/api/account-access/grant", post(handlers::account_access::grant))
        .route(
            "/api/account-access/{user_id}/{account_id}",
            delete(handlers::account_access::revoke),
        )
        .route(
            "/api/my/accessible-accounts",
            get(handlers::account_access::my_accessible_accounts),
        )
        .route("/api/accounts/discover", post(handlers::cloud_account::discover))
        .route("/api/accounts/sync", post(handlers::cloud_account::sync))
        .route(
            "/api/accounts/{id}/test",
            post(handlers::cloud_account::test_connection),
        )
        .route(
            "/api/accounts/{id}/discover-org",
            post(handlers::cloud_account::discover_org),
        )
        .route("/api/accounts/seed-mock", post(handlers::cloud_account::seed_mock))
        // Approvals
        .route("/api/approvals", get(handlers::approval::list))
        .route("/api/approvals/{id}/approve", post(handlers::approval::approve))
        .route("/api/approvals/{id}/reject", post(handlers::approval::reject))
        // Channels
        .route(
            "/api/channels",
            get(handlers::channel::list).post(handlers::channel::create),
        )
        .route(
            "/api/channels/{id}",
            put(handlers::channel::update).delete(handlers::channel::delete),
        )
        // Jira (proxy to Jira Cloud API — used by AI agent)
        .route("/api/jira/create", post(handlers::jira::create_issue))
        .route("/api/jira/{key}/transition", post(handlers::jira::transition_issue))
        .route("/api/jira/{key}/comment", post(handlers::jira::add_comment))
        .route("/api/jira/{key}", get(handlers::jira::get_issue))
        // Clusters
        .route(
            "/api/clusters",
            get(handlers::cluster::list).post(handlers::cluster::create),
        )
        .route("/api/clusters/discover", post(handlers::cluster::discover))
        .route(
            "/api/clusters/{id}",
            put(handlers::cluster::update).delete(handlers::cluster::delete),
        )
        // Service Topology (real-time K8s graph)
        .route("/api/topology", get(handlers::topology::get_topology))
        // Rollouts (Argo Rollouts integration)
        .route("/api/clusters/{id}/rollouts", get(handlers::rollout::list_rollouts))
        .route(
            "/api/clusters/{id}/rollouts/{ns}/{name}",
            get(handlers::rollout::get_rollout),
        )
        .route(
            "/api/clusters/{id}/rollouts/{ns}/{name}/analysis",
            get(handlers::rollout::list_analysis_runs),
        )
        .route(
            "/api/clusters/{id}/rollouts/{ns}/{name}/promote",
            post(handlers::rollout::promote),
        )
        .route(
            "/api/clusters/{id}/rollouts/{ns}/{name}/rollback",
            post(handlers::rollout::rollback),
        )
        .route(
            "/api/clusters/{id}/rollouts/{ns}/{name}/strategy",
            post(handlers::rollout::change_strategy),
        )
        // Deployment events (audit log)
        .route("/api/deployment-events", get(handlers::rollout::list_events))
        // MCP Rollout endpoint (JSON-RPC from Claude CLI)
        .route("/api/mcp/rollouts", post(handlers::mcp_rollout::handle))
        // MCP SLO endpoint (JSON-RPC — slo_query + slo_forecast for the agent)
        .route("/api/mcp/slo", post(handlers::mcp_slo::handle))
        // Resources / Security Insights
        .route("/api/resources", get(handlers::resource::list))
        .route("/api/resources/scan", post(handlers::resource::scan))
        .route("/api/resources/scans", get(handlers::resource::list_scans))
        .route("/api/resources/scans/{id}", get(handlers::resource::get_scan))
        .route("/api/resources/findings", get(handlers::resource::list_findings))
        .route("/api/resources/dashboard", get(handlers::resource::dashboard))
        .route(
            "/api/resources/screener/status",
            get(handlers::resource::screener_status),
        )
        .route(
            "/api/resources/screener/setup",
            post(handlers::resource::setup_screener),
        )
        // Issues
        .route("/api/issues", get(handlers::issue::list))
        .route("/api/issues/count", get(handlers::issue::count))
        .route(
            "/api/issues/{id}",
            get(handlers::issue::get).put(handlers::issue::update),
        )
        .route("/api/issues/{id}/rca", post(handlers::issue::start_rca))
        .route("/api/issues/{id}/rca/status", get(handlers::issue::rca_status))
        // Promote an issue into a full Incident (see `handlers::issue::promote_to_incident`)
        .route(
            "/api/issues/{id}/promote",
            post(handlers::issue::promote_to_incident),
        )
        // Incident Command
        .route(
            "/api/incidents",
            get(handlers::incident::list).post(handlers::incident::create),
        )
        .route("/api/incidents/active", get(handlers::incident::list_active))
        .route(
            "/api/incidents/{id}",
            get(handlers::incident::get).patch(handlers::incident::update),
        )
        .route(
            "/api/incidents/{id}/transition",
            post(handlers::incident::transition),
        )
        .route(
            "/api/incidents/{id}/severity",
            post(handlers::incident::change_severity),
        )
        .route(
            "/api/incidents/{id}/participants",
            post(handlers::incident::add_participant),
        )
        .route(
            "/api/incidents/{id}/participants/{user_id}/{role}",
            delete(handlers::incident::remove_participant),
        )
        .route(
            "/api/incidents/{id}/timeline",
            get(handlers::incident::list_timeline)
                .post(handlers::incident::create_timeline_note),
        )
        .route(
            "/api/incidents/{id}/timeline/stream",
            get(handlers::incident::stream_timeline),
        )
        .route(
            "/api/incidents/{id}/updates",
            get(handlers::incident::list_updates)
                .post(handlers::incident::create_update),
        )
        .route(
            "/api/incidents/{id}/postmortem",
            get(handlers::incident::get_postmortem)
                .patch(handlers::incident::update_postmortem),
        )
        .route(
            "/api/incidents/{id}/postmortem/draft",
            post(handlers::incident::draft_postmortem),
        )
        .route(
            "/api/incidents/{id}/postmortem/publish",
            post(handlers::incident::publish_postmortem),
        )
        // MCP Incident endpoint (JSON-RPC for agent tool calls)
        .route("/api/mcp/incidents", post(handlers::mcp_incident::handle))
        // Knowledge
        .route(
            "/api/knowledge",
            get(handlers::knowledge::list).post(handlers::knowledge::create),
        )
        .route(
            "/api/knowledge/{id}",
            put(handlers::knowledge::update).delete(handlers::knowledge::delete),
        )
        .route("/api/knowledge/sync", post(handlers::knowledge::sync))
        // Scheduled Jobs
        .route(
            "/api/scheduled-jobs",
            get(handlers::scheduled_job::list).post(handlers::scheduled_job::create),
        )
        .route(
            "/api/scheduled-jobs/{id}",
            put(handlers::scheduled_job::update).delete(handlers::scheduled_job::delete),
        )
        .route("/api/scheduled-jobs/{id}/runs", get(handlers::scheduled_job::list_runs))
        .route(
            "/api/scheduled-jobs/{id}/run",
            post(handlers::scheduled_job::trigger_run),
        )
        .route("/api/job-runs/{id}", get(handlers::scheduled_job::get_run))
        // Pipeline Repos
        .route(
            "/api/pipeline/repos",
            get(handlers::pipeline::list).post(handlers::pipeline::create),
        )
        .route(
            "/api/pipeline/repos/test",
            post(handlers::pipeline::test_connection_inline),
        )
        .route(
            "/api/pipeline/repos/{id}",
            put(handlers::pipeline::update).delete(handlers::pipeline::delete),
        )
        .route(
            "/api/pipeline/repos/{id}/test",
            post(handlers::pipeline::test_connection),
        )
        // Telemetry
        .route(
            "/api/telemetry",
            get(handlers::telemetry::list).post(handlers::telemetry::create),
        )
        .route(
            "/api/telemetry/{id}",
            put(handlers::telemetry::update).delete(handlers::telemetry::delete),
        )
        .route("/api/telemetry/test", post(handlers::telemetry::test_connection))
        // Providers (LLM model config)
        .route(
            "/api/providers",
            get(handlers::provider::list).post(handlers::provider::create),
        )
        .route("/api/providers/types", get(handlers::provider::available_types))
        .route(
            "/api/providers/{id}",
            put(handlers::provider::update).delete(handlers::provider::delete),
        )
        // MCP Servers
        .route("/api/mcp", get(handlers::mcp::list).post(handlers::mcp::create))
        .route("/api/mcp/test", post(handlers::mcp::test))
        .route("/api/mcp/{id}/tools", get(handlers::mcp::list_tools))
        // GraphRAG proxy (bbox lookup + PDF presigned URL)
        .route("/api/graphrag/bbox", post(handlers::mcp::graphrag_bbox))
        .route("/api/graphrag/pdf-url", post(handlers::mcp::graphrag_pdf_url))
        .route(
            "/api/graphrag/documents/{context_id}",
            get(handlers::mcp::graphrag_documents),
        )
        .route(
            "/api/mcp/{id}",
            put(handlers::mcp::update).delete(handlers::mcp::delete),
        )
        // Chat (SSE streaming)
        .route("/api/chat", post(handlers::chat::stream))
        .route("/api/chat/sessions", get(handlers::chat::list_sessions))
        .route("/api/chat/sessions/{session_id}/messages", get(handlers::chat::get_messages))
        .route("/api/chat/workspace", get(handlers::chat::workspace_list))
        .route(
            "/api/chat/workspace/{*filepath}",
            get(handlers::chat::workspace_download).delete(handlers::chat::workspace_delete),
        )
        // Dashboard
        .route("/api/dashboard/stats", get(handlers::dashboard::stats))
        // Catalog (W1 — entity CRUD + per-entity relations listing)
        .route(
            "/api/catalog/entities",
            get(handlers::catalog::list).post(handlers::catalog::create),
        )
        .route(
            "/api/catalog/entities/{id}",
            get(handlers::catalog::get)
                .put(handlers::catalog::update)
                .delete(handlers::catalog::delete),
        )
        .route(
            "/api/catalog/entities/{id}/relations",
            get(handlers::catalog::list_relations),
        )
        .route(
            "/api/catalog/entities/{id}/graph",
            get(handlers::catalog::get_graph),
        )
        // Catalog (W2 — YAML import + K8s auto-discovery)
        .route(
            "/api/catalog/import/yaml",
            post(handlers::catalog::import_yaml),
        )
        .route(
            "/api/catalog/discover/k8s",
            post(handlers::catalog::discover_k8s),
        )
        // Change events (W10 Joint Integration — global change stream)
        .route(
            "/api/change-events",
            get(handlers::change_events::list),
        )
        // SLO engine
        .route(
            "/api/slos",
            get(handlers::slo::list).post(handlers::slo::create),
        )
        .route("/api/slos/preview", post(handlers::slo::preview))
        .route(
            "/api/slos/{id}",
            get(handlers::slo::get)
                .put(handlers::slo::update)
                .delete(handlers::slo::delete),
        )
        .route("/api/slos/{id}/enable", post(handlers::slo::enable))
        .route("/api/slos/{id}/disable", post(handlers::slo::disable))
        .route("/api/slos/{id}/sli", get(handlers::slo::sli))
        .route("/api/slos/{id}/budget", get(handlers::slo::budget))
        .route(
            "/api/slos/{id}/budget/history",
            get(handlers::slo::budget_history),
        )
        .route("/api/slos/{id}/sync-rules", post(handlers::slo::sync_rules))
        .layer(axum_middleware::from_fn_with_state(
            jwt_secret,
            middleware::auth::auth_middleware,
        ));

    public_routes.merge(protected_routes).with_state(state)
}

/// Seed a default admin user if no users exist in the database
async fn seed_admin_user(pool: &sqlx::PgPool) {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .unwrap_or((0,));

    if count.0 == 0 {
        let password_hash = bcrypt::hash("admin123", 10).expect("Failed to hash default password");

        let result = sqlx::query(
            r#"INSERT INTO users (username, password_hash, role, email)
               VALUES ('admin', $1, 'super_admin', 'admin@ops.local')"#,
        )
        .bind(&password_hash)
        .execute(pool)
        .await;

        match result {
            Ok(_) => {
                tracing::info!("Default admin user created (username: admin, password: admin123)");
                tracing::warn!("Change the default password immediately!");
            }
            Err(e) => {
                tracing::warn!("Failed to seed admin user: {}", e);
            }
        }
    }
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down..."),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down..."),
    }
}
