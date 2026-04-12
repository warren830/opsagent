use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::scheduled_job::ScheduledJob;

/// Main scheduler loop — runs every 60 seconds, evaluates cron expressions,
/// and dispatches jobs whose schedule matches the current minute.
pub async fn run_scheduler(pool: PgPool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.tick().await; // skip first immediate tick

    loop {
        interval.tick().await;
        if let Err(e) = tick(&pool).await {
            tracing::error!("Scheduler tick failed: {}", e);
        }
    }
}

/// One scheduler tick: find enabled jobs whose cron matches now, dispatch each.
async fn tick(pool: &PgPool) -> anyhow::Result<()> {
    let jobs = sqlx::query_as::<_, ScheduledJob>("SELECT * FROM scheduled_jobs WHERE enabled = true")
        .fetch_all(pool)
        .await?;

    let now = Utc::now();

    for job in &jobs {
        // Parse cron and check if it matches the current minute
        let cron = match croner::Cron::new(&job.cron_expression).parse() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Invalid cron '{}' for job {}: {}", job.cron_expression, job.id, e);
                continue;
            }
        };

        // Find next occurrence from 1 minute ago — if it falls within this minute, fire
        let one_min_ago = now - chrono::Duration::seconds(60);
        let next = cron.find_next_occurrence(&one_min_ago, false);
        match next {
            Ok(next_time) => {
                let diff = (now - next_time).num_seconds().abs();
                if diff > 59 {
                    continue;
                }
            }
            Err(_) => continue,
        }

        // Skip if already ran within the last 59 seconds (prevent double-fire)
        if let Some(last_run) = job.last_run_at {
            let since_last = (now - last_run).num_seconds();
            if since_last < 59 {
                continue;
            }
        }

        tracing::info!("Scheduler triggering job '{}' ({})", job.name, job.id);

        // Create run record
        let run_id = match sqlx::query_scalar::<_, Uuid>(
            r#"INSERT INTO job_runs (job_id, status, trigger, tenant_id)
               VALUES ($1, 'pending', 'scheduled', $2)
               RETURNING id"#,
        )
        .bind(job.id)
        .bind(job.tenant_id)
        .fetch_one(pool)
        .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to create run record for job {}: {}", job.id, e);
                continue;
            }
        };

        // Update last_run_at
        let _ = sqlx::query("UPDATE scheduled_jobs SET last_run_at = NOW() WHERE id = $1")
            .bind(job.id)
            .execute(pool)
            .await;

        // Dispatch in background
        let pool_clone = pool.clone();
        let job_clone = job.clone();
        tokio::spawn(async move {
            execute_job(&pool_clone, &job_clone, run_id).await;
        });
    }

    Ok(())
}

/// Execute a single job, updating the run record with results.
/// All jobs are Agent type — query is sent to `claude -p`.
pub async fn execute_job(pool: &PgPool, job: &ScheduledJob, run_id: Uuid) {
    // Mark as running
    let _ = sqlx::query("UPDATE job_runs SET status = 'running', started_at = NOW() WHERE id = $1")
        .bind(run_id)
        .execute(pool)
        .await;

    let query = job.query.as_deref().unwrap_or("");
    let result = if query.is_empty() {
        Err("Job has no query".to_string())
    } else {
        run_claude_cli(query).await
    };

    // Update run record
    match result {
        Ok(output) => {
            let summary = output.summary.unwrap_or_default();
            let _ = sqlx::query(
                r#"UPDATE job_runs SET
                   status = 'success',
                   finished_at = NOW(),
                   duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at))::bigint * 1000,
                   summary = $2,
                   output = $3,
                   exit_code = $4
                   WHERE id = $1"#,
            )
            .bind(run_id)
            .bind(&summary)
            .bind(&output.output)
            .bind(output.exit_code)
            .execute(pool)
            .await;

            tracing::info!("Job '{}' run {} completed: {}", job.name, run_id, summary);
        }
        Err(err) => {
            let _ = sqlx::query(
                r#"UPDATE job_runs SET
                   status = 'failed',
                   finished_at = NOW(),
                   duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at))::bigint * 1000,
                   error = $2
                   WHERE id = $1"#,
            )
            .bind(run_id)
            .bind(&err)
            .execute(pool)
            .await;

            tracing::error!("Job '{}' run {} failed: {}", job.name, run_id, err);
        }
    }
}

struct JobOutput {
    summary: Option<String>,
    output: Option<String>,
    exit_code: Option<i32>,
}

/// Invoke `claude -p "<query>"` and capture stdout as the result.
async fn run_claude_cli(prompt: &str) -> Result<JobOutput, String> {
    let claude_bin = std::env::var("CLAUDE_BIN").unwrap_or_else(|_| {
        // Try to find claude in common paths when not in PATH
        for path in ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"] {
            if std::path::Path::new(path).exists() {
                return path.to_string();
            }
        }
        "claude".to_string()
    });

    let mut cmd = tokio::process::Command::new(&claude_bin);
    cmd.args([
        "-p",
        prompt,
        "--output-format",
        "text",
        "--permission-mode",
        super::claude::AgentPermission::Bypass.cli_flag(),
    ]);

    // Remove Claude Code nesting detection vars (causes subprocess to fail)
    cmd.env_remove("CLAUDECODE");
    cmd.env_remove("CLAUDE_CODE_SSE_PORT");
    cmd.env_remove("CLAUDE_CODE_ENTRYPOINT");
    cmd.env_remove("CLAUDE_CODE_EXECPATH");
    cmd.env_remove("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS");

    // Forward Bedrock auth: AWS_PROFILE + AWS_BEARER_TOKEN_BEDROCK
    if let Ok(profile) = std::env::var("AWS_PROFILE") {
        cmd.env("AWS_PROFILE", &profile);
    }
    if let Ok(token) = std::env::var("AWS_BEARER_TOKEN_BEDROCK") {
        cmd.env("AWS_BEARER_TOKEN_BEDROCK", &token);
    }

    let timeout_secs = std::env::var("CLAUDE_JOB_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300u64);

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);

    let child = cmd.spawn().map_err(|e| format!("Failed to spawn claude CLI: {e}"))?;

    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), child.wait_with_output())
        .await
        .map_err(|_| format!("claude CLI timed out after {timeout_secs}s"))?
        .map_err(|e| format!("claude CLI error: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    tracing::debug!(
        "claude exit={exit_code} stdout_len={} stderr_len={}",
        stdout.len(),
        stderr.len()
    );
    if !stderr.is_empty() {
        tracing::debug!("claude stderr: {}", &stderr[..stderr.len().min(500)]);
    }

    if !output.status.success() {
        // Include both stderr and stdout in error for diagnosis
        let detail = if stderr.is_empty() { &stdout } else { &stderr };
        return Err(format!(
            "claude exited with code {exit_code}: {}",
            detail.chars().take(2000).collect::<String>()
        ));
    }

    // Extract first line as summary (up to 200 chars)
    let summary = stdout.lines().next().map(|l| l.chars().take(200).collect::<String>());

    Ok(JobOutput {
        summary,
        output: Some(stdout),
        exit_code: Some(exit_code),
    })
}
