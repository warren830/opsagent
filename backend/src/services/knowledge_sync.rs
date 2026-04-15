use sqlx::PgPool;
use serde::Serialize;

use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::services::jira::JiraClient;

#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub source: String,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub errors: Vec<String>,
}

/// Sync Jira issues matching a JQL filter into knowledge_files.
///
/// Uses a simple upsert strategy: if a row with the same (source, source_id) exists
/// we always overwrite it and count as "updated". This avoids timezone-parsing issues
/// when comparing the Jira `updated` string against the DB TIMESTAMPTZ column.
pub async fn sync_jira(
    pool: &PgPool,
    auth_user: &AuthUser,
    client: &JiraClient,
    jql: &str,
    max_results: usize,
) -> AppResult<SyncResult> {
    let issues = client.search_issues(jql, max_results).await?;
    let mut result = SyncResult {
        source: "jira".into(),
        added: 0,
        updated: 0,
        unchanged: 0,
        errors: vec![],
    };

    for issue in &issues {
        // Build markdown content from the issue
        let content = format!(
            "# {key}: {summary}\n\n\
             **Labels**: {labels}\n\
             **Updated**: {updated}\n\
             **Link**: [{key}]({url})\n\n\
             ## Description\n\n\
             {desc}\n\n\
             ## Comments\n\n\
             {comments}",
            key = issue.key,
            summary = issue.summary,
            labels = if issue.labels.is_empty() {
                "(none)".to_string()
            } else {
                issue.labels.join(", ")
            },
            updated = issue.updated,
            url = issue.url,
            desc = issue.description_text,
            comments = if issue.comments.is_empty() {
                "(none)".to_string()
            } else {
                issue
                    .comments
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("### Comment {}\n{}", i + 1, c))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            },
        );
        let filename = format!("jira-{}.md", issue.key);
        let size_bytes = content.len() as i64;
        let tenant_id = auth_user.tenant_id;

        // Check if already exists by (source, source_id)
        let existing: Option<(bool,)> = sqlx::query_as(
            "SELECT true FROM knowledge_files WHERE source = 'jira' AND source_id = $1",
        )
        .bind(&issue.key)
        .fetch_optional(pool)
        .await?;

        match existing {
            Some(_) => {
                // Always update — avoids timezone comparison issues
                if let Err(e) = sqlx::query(
                    "UPDATE knowledge_files \
                     SET filename = $1, content = $2, size_bytes = $3, source_url = $4, \
                         source_updated_at = NOW(), updated_at = NOW() \
                     WHERE source = 'jira' AND source_id = $5",
                )
                .bind(&filename)
                .bind(&content)
                .bind(size_bytes)
                .bind(&issue.url)
                .bind(&issue.key)
                .execute(pool)
                .await
                {
                    result.errors.push(format!("Update {}: {}", issue.key, e));
                    continue;
                }
                result.updated += 1;
            }
            None => {
                if let Err(e) = sqlx::query(
                    "INSERT INTO knowledge_files \
                     (filename, content, size_bytes, mime_type, source, source_id, source_url, \
                      source_updated_at, tenant_id, created_by) \
                     VALUES ($1, $2, $3, 'text/markdown', 'jira', $4, $5, NOW(), $6, $7)",
                )
                .bind(&filename)
                .bind(&content)
                .bind(size_bytes)
                .bind(&issue.key)
                .bind(&issue.url)
                .bind(tenant_id)
                .bind(auth_user.user_id)
                .execute(pool)
                .await
                {
                    result.errors.push(format!("Insert {}: {}", issue.key, e));
                    continue;
                }
                result.added += 1;
            }
        }
    }

    tracing::info!(
        "Jira sync complete: added={}, updated={}, errors={}",
        result.added,
        result.updated,
        result.errors.len()
    );

    Ok(result)
}

/// Sync Confluence pages from a space into knowledge_files.
///
/// Same upsert strategy as `sync_jira`.
pub async fn sync_confluence(
    pool: &PgPool,
    auth_user: &AuthUser,
    client: &JiraClient,
    space_key: &str,
    max_pages: usize,
) -> AppResult<SyncResult> {
    let pages = client.get_confluence_pages(space_key, max_pages).await?;
    let mut result = SyncResult {
        source: "confluence".into(),
        added: 0,
        updated: 0,
        unchanged: 0,
        errors: vec![],
    };

    for page in &pages {
        let filename = format!("confluence-{}.md", page.id);
        let content = format!("# {}\n\n{}", page.title, page.body_markdown);
        let size_bytes = content.len() as i64;
        let tenant_id = auth_user.tenant_id;

        let existing: Option<(bool,)> = sqlx::query_as(
            "SELECT true FROM knowledge_files WHERE source = 'confluence' AND source_id = $1",
        )
        .bind(&page.id)
        .fetch_optional(pool)
        .await?;

        match existing {
            Some(_) => {
                if let Err(e) = sqlx::query(
                    "UPDATE knowledge_files \
                     SET filename = $1, content = $2, size_bytes = $3, source_url = $4, \
                         source_updated_at = NOW(), updated_at = NOW() \
                     WHERE source = 'confluence' AND source_id = $5",
                )
                .bind(&filename)
                .bind(&content)
                .bind(size_bytes)
                .bind(&page.url)
                .bind(&page.id)
                .execute(pool)
                .await
                {
                    result
                        .errors
                        .push(format!("Update confluence-{}: {}", page.id, e));
                    continue;
                }
                result.updated += 1;
            }
            None => {
                if let Err(e) = sqlx::query(
                    "INSERT INTO knowledge_files \
                     (filename, content, size_bytes, mime_type, source, source_id, source_url, \
                      source_updated_at, tenant_id, created_by) \
                     VALUES ($1, $2, $3, 'text/markdown', 'confluence', $4, $5, NOW(), $6, $7)",
                )
                .bind(&filename)
                .bind(&content)
                .bind(size_bytes)
                .bind(&page.id)
                .bind(&page.url)
                .bind(tenant_id)
                .bind(auth_user.user_id)
                .execute(pool)
                .await
                {
                    result
                        .errors
                        .push(format!("Insert confluence-{}: {}", page.id, e));
                    continue;
                }
                result.added += 1;
            }
        }
    }

    tracing::info!(
        "Confluence sync complete: added={}, updated={}, errors={}",
        result.added,
        result.updated,
        result.errors.len()
    );

    Ok(result)
}
