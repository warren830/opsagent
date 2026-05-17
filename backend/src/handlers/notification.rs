use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, Sse},
    Json,
};
use std::convert::Infallible;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::notification::{Notification, NotificationListQuery};
use crate::AppState;

type SseEventStream = Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;

/// GET /api/notifications
pub async fn list(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Query(query): Query<NotificationListQuery>,
) -> AppResult<Json<Vec<Notification>>> {
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    let rows = sqlx::query_as::<_, Notification>(
        r#"SELECT * FROM notifications
           WHERE user_id = $1
           ORDER BY created_at DESC
           LIMIT $2 OFFSET $3"#,
    )
    .bind(auth_user.user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

/// GET /api/notifications/unread-count
pub async fn unread_count(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false")
        .bind(auth_user.user_id)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({ "count": count.0 })))
}

/// POST /api/notifications/:id/read
pub async fn mark_read(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Notification>> {
    let row = sqlx::query_as::<_, Notification>(
        r#"UPDATE notifications
           SET is_read = true
           WHERE id = $1 AND user_id = $2
           RETURNING *"#,
    )
    .bind(id)
    .bind(auth_user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

    Ok(Json(row))
}

/// POST /api/notifications/read-all
pub async fn mark_all_read(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query("UPDATE notifications SET is_read = true WHERE user_id = $1 AND is_read = false")
        .bind(auth_user.user_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(serde_json::json!({ "updated": result.rows_affected() })))
}

/// GET /api/notifications/stream — SSE driven by broadcast channel.
/// Subscribes to AppState.notification_tx; only forwards notifications
/// whose user_id matches the authenticated user.
pub async fn stream(
    auth_user: axum::Extension<AuthUser>,
    State(state): State<AppState>,
) -> Sse<axum::response::sse::KeepAliveStream<SseEventStream>> {
    let user_id = auth_user.user_id;
    let pool = state.pool.clone();
    let mut rx = state.notification_tx.subscribe();

    let sse_stream = async_stream::stream! {
        // Send initial unread count so the client has a baseline immediately.
        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
        let init = serde_json::to_string(&serde_json::json!({ "unread_count": unread }))
            .unwrap_or_default();
        yield Ok::<_, Infallible>(Event::default().event("notification").data(init));

        // Forward broadcast messages that belong to this user.
        loop {
            match rx.recv().await {
                Ok(notif) if notif.user_id == user_id => {
                    if let Ok(data) = serde_json::to_string(&notif) {
                        yield Ok(Event::default().event("notification").data(data));
                    }
                }
                Ok(_) => {} // belongs to a different user — skip
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Subscriber fell behind; send a refresh hint so the client re-fetches.
                    yield Ok(Event::default().event("refresh").data("lagged"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(Box::pin(sse_stream) as SseEventStream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}
