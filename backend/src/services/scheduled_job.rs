use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::scheduled_job::{
    CreateScheduledJobRequest, JobRun, ScheduledJob, UpdateScheduledJobRequest,
};

/// Check whether the authenticated user can access the given job.
fn check_job_access(auth_user: &AuthUser, job: &ScheduledJob) -> Result<(), AppError> {
    if auth_user.is_super_admin() {
        return Ok(());
    }
    let has_access = job.user_id == Some(auth_user.user_id)
        || (job.visibility == "public" && job.tenant_id == auth_user.tenant_id);
    if !has_access {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }
    Ok(())
}

/// List scheduled jobs visible to the authenticated user.
/// Super-admin sees all; others see own private + tenant public jobs.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<ScheduledJob>> {
    let rows = if auth_user.is_super_admin() {
        sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs ORDER BY name")
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, ScheduledJob>(
            r#"SELECT * FROM scheduled_jobs
               WHERE (user_id = $1) OR (user_id IS NULL AND tenant_id IS NOT DISTINCT FROM $2)
               ORDER BY name"#,
        )
        .bind(auth_user.user_id)
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

/// Create a new scheduled job.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    req: CreateScheduledJobRequest,
) -> AppResult<ScheduledJob> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    if req.cron_expression.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Cron expression is required".to_string(),
        ));
    }

    let job_type = match req.job_type.as_str() {
        "builtin" | "agent" | "skill" => req.job_type.clone(),
        _ => "agent".to_string(),
    };

    if job_type == "skill" && req.skill_path.is_none() {
        return Err(AppError::BadRequest(
            "Skill path is required for skill jobs".to_string(),
        ));
    }

    let visibility = match req.visibility.as_str() {
        "public" | "private" => req.visibility.clone(),
        _ => "public".to_string(),
    };

    let tenant_id = auth_user.tenant_id;
    let user_id = if visibility == "private" {
        Some(auth_user.user_id)
    } else {
        None
    };

    let row = sqlx::query_as::<_, ScheduledJob>(
        r#"INSERT INTO scheduled_jobs (name, cron_expression, timezone, query, enabled, auto_jira, targets,
           tenant_id, user_id, created_by, visibility, job_type, skill_path, skill_params)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
           RETURNING *"#,
    )
    .bind(&req.name)
    .bind(&req.cron_expression)
    .bind(&req.timezone)
    .bind(&req.query)
    .bind(req.enabled)
    .bind(req.auto_jira)
    .bind(&req.targets)
    .bind(tenant_id)
    .bind(user_id)
    .bind(auth_user.user_id)
    .bind(&visibility)
    .bind(&job_type)
    .bind(&req.skill_path)
    .bind(&req.skill_params)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Update an existing scheduled job (with access check).
pub async fn update(
    pool: &PgPool,
    auth_user: &AuthUser,
    id: Uuid,
    req: UpdateScheduledJobRequest,
) -> AppResult<ScheduledJob> {
    let existing = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    check_job_access(auth_user, &existing)?;

    let row = sqlx::query_as::<_, ScheduledJob>(
        r#"UPDATE scheduled_jobs SET
           name = COALESCE($2, name),
           cron_expression = COALESCE($3, cron_expression),
           timezone = COALESCE($4, timezone),
           query = COALESCE($5, query),
           enabled = COALESCE($6, enabled),
           auto_jira = COALESCE($7, auto_jira),
           targets = COALESCE($8, targets),
           job_type = COALESCE($9, job_type),
           skill_path = COALESCE($10, skill_path),
           skill_params = COALESCE($11, skill_params),
           updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.cron_expression)
    .bind(&req.timezone)
    .bind(&req.query)
    .bind(req.enabled)
    .bind(req.auto_jira)
    .bind(&req.targets)
    .bind(&req.job_type)
    .bind(&req.skill_path)
    .bind(&req.skill_params)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    Ok(row)
}

/// Delete a scheduled job (with access check).
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    let existing = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    check_job_access(auth_user, &existing)?;

    sqlx::query("DELETE FROM scheduled_jobs WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

/// List execution runs for a job (with access check).
pub async fn list_runs(
    pool: &PgPool,
    auth_user: &AuthUser,
    job_id: Uuid,
    limit: i64,
) -> AppResult<Vec<JobRun>> {
    let job = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    check_job_access(auth_user, &job)?;

    let rows = sqlx::query_as::<_, JobRun>(
        "SELECT * FROM job_runs WHERE job_id = $1 ORDER BY started_at DESC NULLS LAST LIMIT $2",
    )
    .bind(job_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Fetch a single job run by ID.
pub async fn get_run(pool: &PgPool, run_id: Uuid) -> AppResult<JobRun> {
    let run = sqlx::query_as::<_, JobRun>("SELECT * FROM job_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Job run not found".to_string()))?;

    Ok(run)
}

/// Trigger a manual job run. Returns both the created run and the job
/// (the caller needs the job to dispatch background execution).
pub async fn trigger_run(
    pool: &PgPool,
    auth_user: &AuthUser,
    job_id: Uuid,
) -> AppResult<(JobRun, ScheduledJob)> {
    let job = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Scheduled job not found".to_string()))?;

    check_job_access(auth_user, &job)?;

    let run = sqlx::query_as::<_, JobRun>(
        r#"INSERT INTO job_runs (job_id, status, trigger, tenant_id)
           VALUES ($1, 'pending', 'manual', $2)
           RETURNING *"#,
    )
    .bind(job_id)
    .bind(job.tenant_id)
    .fetch_one(pool)
    .await?;

    Ok((run, job))
}
