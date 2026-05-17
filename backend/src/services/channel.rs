use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::channel::{Channel, ChannelWithTenants, CreateChannelRequest, UpdateChannelRequest};
use crate::services::common::require_non_empty;

/// Fetch tenant_ids for a batch of channel IDs from the join table.
async fn fetch_tenant_ids(pool: &PgPool, channel_ids: &[Uuid]) -> sqlx::Result<std::collections::HashMap<Uuid, Vec<Uuid>>> {
    if channel_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT channel_id, tenant_id FROM channel_tenant_assignments WHERE channel_id = ANY($1)",
    )
    .bind(channel_ids)
    .fetch_all(pool)
    .await?;

    let mut map: std::collections::HashMap<Uuid, Vec<Uuid>> = std::collections::HashMap::new();
    for (cid, tid) in rows {
        map.entry(cid).or_default().push(tid);
    }
    Ok(map)
}

/// Attach tenant_ids to a list of channels.
async fn with_tenants(pool: &PgPool, channels: Vec<Channel>) -> AppResult<Vec<ChannelWithTenants>> {
    let ids: Vec<Uuid> = channels.iter().map(|c| c.id).collect();
    let mut map = fetch_tenant_ids(pool, &ids).await?;
    Ok(channels
        .into_iter()
        .map(|c| {
            let tenant_ids = map.remove(&c.id).unwrap_or_default();
            ChannelWithTenants { channel: c, tenant_ids }
        })
        .collect())
}

/// Replace all tenant assignments for a channel (transactional DELETE + INSERT).
async fn set_tenant_ids(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    channel_id: Uuid,
    tenant_ids: &[Uuid],
) -> AppResult<()> {
    sqlx::query("DELETE FROM channel_tenant_assignments WHERE channel_id = $1")
        .bind(channel_id)
        .execute(&mut **tx)
        .await?;
    for &tid in tenant_ids {
        sqlx::query(
            "INSERT INTO channel_tenant_assignments (channel_id, tenant_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(channel_id)
        .bind(tid)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// List channels visible to the authenticated user, with their tenant_ids.
/// Super admins see all channels; other users see channels owned by or shared with their tenant.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<ChannelWithTenants>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Channel>("SELECT * FROM channels ORDER BY name")
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, Channel>(
            r#"SELECT DISTINCT c.* FROM channels c
               LEFT JOIN channel_tenant_assignments cta ON cta.channel_id = c.id
               WHERE c.tenant_id = $1 OR cta.tenant_id = $1
               ORDER BY c.name"#,
        )
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    };
    with_tenants(pool, rows).await
}

/// Create a new channel and set its tenant assignments.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateChannelRequest,
) -> AppResult<ChannelWithTenants> {
    require_non_empty(&req.name, "Name")?;
    require_non_empty(&req.platform, "Platform")?;

    let mut tx = pool.begin().await?;

    let channel = sqlx::query_as::<_, Channel>(
        r#"INSERT INTO channels (platform, name, credentials, settings, enabled, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(&req.platform)
    .bind(&req.name)
    .bind(&req.credentials)
    .bind(&req.settings)
    .bind(req.enabled)
    .bind(auth_user.tenant_id)
    .fetch_one(&mut *tx)
    .await?;

    let tenant_ids = if req.tenant_ids.is_empty() {
        // Default: assign to creator's own tenant
        auth_user.tenant_id.map(|t| vec![t]).unwrap_or_default()
    } else {
        req.tenant_ids.clone()
    };
    set_tenant_ids(&mut tx, channel.id, &tenant_ids).await?;
    tx.commit().await?;

    Ok(ChannelWithTenants { channel, tenant_ids })
}

/// Update an existing channel. Non-admin users can only update channels
/// belonging to their own tenant.
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateChannelRequest,
) -> AppResult<ChannelWithTenants> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let mut tx = pool.begin().await?;

    let channel = sqlx::query_as::<_, Channel>(
        r#"UPDATE channels SET
           platform  = COALESCE($2, platform),
           name      = COALESCE($3, name),
           credentials = COALESCE($4, credentials),
           settings  = COALESCE($5, settings),
           enabled   = COALESCE($6, enabled),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.platform)
    .bind(&req.name)
    .bind(&req.credentials)
    .bind(&req.settings)
    .bind(req.enabled)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    let tenant_ids = if let Some(ids) = req.tenant_ids {
        set_tenant_ids(&mut tx, id, &ids).await?;
        ids
    } else {
        // No change requested — fetch current assignments
        sqlx::query_scalar::<_, Uuid>(
            "SELECT tenant_id FROM channel_tenant_assignments WHERE channel_id = $1",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?
    };

    tx.commit().await?;
    Ok(ChannelWithTenants { channel, tenant_ids })
}

/// Delete a channel by ID.
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    if !auth_user.is_super_admin() {
        let existing = sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;
        if existing.tenant_id != auth_user.tenant_id {
            return Err(AppError::Forbidden("Access denied".to_string()));
        }
    }

    let result = sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Channel not found".to_string()));
    }

    Ok(())
}
