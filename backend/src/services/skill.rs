use serde::Serialize;
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::AuthUser;
use crate::models::skill::Skill;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DiscoveredSkill {
    pub name: String,
    pub description: Option<String>,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub git_url: String,
    pub skills: Vec<DiscoveredSkill>,
}

// ─── List ───────────────────────────────────────────────────────────────────

/// List skills visible to the current user.
/// Super-admin sees all; others see own private + tenant public.
pub async fn list(pool: &PgPool, auth_user: &AuthUser) -> AppResult<Vec<Skill>> {
    let skills = if auth_user.is_super_admin() {
        sqlx::query_as::<_, Skill>(
            r#"SELECT id, name, description, instructions, git_url, repo_path,
                      visibility, enabled, tenant_id, user_id, created_by, created_at, updated_at
               FROM skills ORDER BY visibility, name"#,
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Skill>(
            r#"SELECT id, name, description, instructions, git_url, repo_path,
                      visibility, enabled, tenant_id, user_id, created_by, created_at, updated_at
               FROM skills
               WHERE (user_id = $1) OR (user_id IS NULL AND tenant_id IS NOT DISTINCT FROM $2)
               ORDER BY visibility, name"#,
        )
        .bind(auth_user.user_id)
        .bind(auth_user.tenant_id)
        .fetch_all(pool)
        .await?
    };

    Ok(skills)
}

// ─── Discover ───────────────────────────────────────────────────────────────

/// Clone a git repo to /tmp, scan for SKILL.md files, return discovered skills.
pub async fn discover(git_url: &str) -> AppResult<DiscoverResponse> {
    let git_url = git_url.trim().to_string();
    if git_url.is_empty() {
        return Err(AppError::BadRequest("git_url is required".to_string()));
    }

    // Clone to temp dir
    let tmp_id = Uuid::new_v4();
    let tmp_dir = PathBuf::from(format!("/tmp/openops-discover-{}", tmp_id));

    let output = Command::new("git")
        .args(["clone", "--depth", "1", &git_url, &tmp_dir.to_string_lossy()])
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to run git clone: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::BadRequest(format!(
            "Git clone failed: {}",
            stderr.trim()
        )));
    }

    let mut discovered = Vec::new();

    // 1) Check root SKILL.md (single-skill repo)
    let root_skill = tmp_dir.join("SKILL.md");
    if root_skill.exists() {
        let (name, desc) = parse_skill_md(&root_skill).await;
        discovered.push(DiscoveredSkill {
            name,
            description: desc,
            path: ".".to_string(),
        });
    }

    // 2) Scan skills/*/ subdirectories
    let skills_dir = tmp_dir.join("skills");
    if skills_dir.is_dir()
        && let Ok(mut entries) = tokio::fs::read_dir(&skills_dir).await
    {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let sub_path = entry.path();
            if sub_path.is_dir() {
                let md = sub_path.join("SKILL.md");
                if md.exists() {
                    let (name, desc) = parse_skill_md(&md).await;
                    let rel = format!(
                        "skills/{}",
                        sub_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    );
                    discovered.push(DiscoveredSkill {
                        name,
                        description: desc,
                        path: rel,
                    });
                }
            }
        }
    }

    // Cleanup tmp
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    if discovered.is_empty() {
        return Err(AppError::BadRequest(
            "No SKILL.md found in repository (checked root and skills/*/)".to_string(),
        ));
    }

    // Sort by name
    discovered.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(DiscoverResponse {
        git_url,
        skills: discovered,
    })
}

// ─── Create (Install) ──────────────────────────────────────────────────────

/// Install selected skills from a git repo into the workspace.
pub async fn create(
    pool: &PgPool,
    auth_user: &AuthUser,
    git_url: &str,
    selected: &[String],
    visibility: &str,
    work_dir: &str,
) -> AppResult<Vec<Skill>> {
    let git_url = git_url.trim().to_string();
    if git_url.is_empty() {
        return Err(AppError::BadRequest("git_url is required".to_string()));
    }
    if selected.is_empty() {
        return Err(AppError::BadRequest(
            "At least one skill must be selected".to_string(),
        ));
    }

    let visibility = match visibility {
        "public" | "private" => visibility.to_string(),
        _ => "private".to_string(),
    };

    let tenant_id = auth_user.tenant_id;
    let user_id = if visibility == "private" {
        Some(auth_user.user_id)
    } else {
        None
    };

    // Clone to temp dir
    let tmp_id = Uuid::new_v4();
    let tmp_dir = PathBuf::from(format!("/tmp/openops-install-{}", tmp_id));

    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            &git_url,
            &tmp_dir.to_string_lossy(),
        ])
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to run git clone: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        return Err(AppError::BadRequest(format!(
            "Git clone failed: {}",
            stderr.trim()
        )));
    }

    let tenant_dir = tenant_id
        .map(|t| t.to_string())
        .unwrap_or_else(|| "global".to_string());

    // Build skills_base with absolute path — create dir first so canonicalize works
    let raw_workspace = PathBuf::from(work_dir);
    let raw_skills_base = raw_workspace.join(&tenant_dir).join("skills");
    if let Err(e) = tokio::fs::create_dir_all(&raw_skills_base).await {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        return Err(AppError::Internal(format!(
            "Failed to create skills directory: {}",
            e
        )));
    }
    // Now canonicalize to get absolute path (dir exists so this should succeed)
    let skills_base = std::fs::canonicalize(&raw_skills_base).unwrap_or_else(|_| {
        std::env::current_dir()
            .map(|cwd| cwd.join(&raw_skills_base))
            .unwrap_or(raw_skills_base)
    });

    let mut installed = Vec::new();

    for selected_path in selected {
        let src = if selected_path == "." {
            tmp_dir.clone()
        } else {
            tmp_dir.join(selected_path)
        };

        if !src.join("SKILL.md").exists() {
            tracing::warn!("Skipping {}: no SKILL.md found", selected_path);
            continue;
        }

        let (name, description) = parse_skill_md(&src.join("SKILL.md")).await;
        let skill_id = Uuid::new_v4();
        // Use skill name as directory name (sanitized)
        let dir_name = name
            .to_lowercase()
            .replace(
                |c: char| !c.is_alphanumeric() && c != '-' && c != '_',
                "-",
            )
            .trim_matches('-')
            .to_string();
        let dir_name = if dir_name.is_empty() {
            skill_id.to_string()
        } else {
            dir_name
        };
        let dest = skills_base.join(&dir_name);

        // Copy the skill directory to workspace
        if let Err(e) = copy_dir_recursive(&src, &dest).await {
            tracing::error!("Failed to copy skill {} to {:?}: {}", name, dest, e);
            continue;
        }

        // Store git_url with fragment to identify sub-skill
        let stored_url = if selected_path == "." {
            git_url.clone()
        } else {
            format!("{}#{}", git_url, selected_path)
        };

        // Insert into DB
        match sqlx::query_as::<_, Skill>(
            r#"INSERT INTO skills (id, name, description, git_url, repo_path, visibility, enabled, tenant_id, user_id, created_by)
               VALUES ($1, $2, $3, $4, $5, $6, true, $7, $8, $9)
               RETURNING id, name, description, instructions, git_url, repo_path,
                         visibility, enabled, tenant_id, user_id, created_by, created_at, updated_at"#,
        )
        .bind(skill_id)
        .bind(&name)
        .bind(&description)
        .bind(&stored_url)
        .bind(dest.to_string_lossy().as_ref())
        .bind(&visibility)
        .bind(tenant_id)
        .bind(user_id)
        .bind(auth_user.user_id)
        .fetch_one(pool)
        .await
        {
            Ok(skill) => {
                tracing::info!("Installed skill '{}' at {:?}", name, dest);
                installed.push(skill);
            }
            Err(e) => {
                tracing::error!("Failed to insert skill '{}': {}", name, e);
                let _ = tokio::fs::remove_dir_all(&dest).await;
            }
        }
    }

    // Cleanup tmp
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    if installed.is_empty() {
        return Err(AppError::Internal(
            "No skills were installed successfully".to_string(),
        ));
    }

    Ok(installed)
}

// ─── Update ─────────────────────────────────────────────────────────────────

/// Re-download a skill from its git_url and update name/description.
pub async fn update(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<Skill> {
    let skill = get_skill_with_access(pool, id, auth_user).await?;

    let (Some(ref repo_path), Some(ref git_url_raw)) = (skill.repo_path, skill.git_url) else {
        return Err(AppError::BadRequest(
            "Skill has no repo_path or git_url, cannot update".to_string(),
        ));
    };

    // Parse git_url#sub_path format
    let (git_url, sub_path) = if let Some(idx) = git_url_raw.find('#') {
        (&git_url_raw[..idx], Some(&git_url_raw[idx + 1..]))
    } else {
        (git_url_raw.as_str(), None)
    };

    // Clone to temp dir
    let tmp_id = Uuid::new_v4();
    let tmp_dir = PathBuf::from(format!("/tmp/openops-update-{}", tmp_id));

    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            git_url,
            &tmp_dir.to_string_lossy(),
        ])
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to run git clone: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        tracing::warn!("git clone failed for skill update {}: {}", id, stderr);
        return Err(AppError::BadRequest(format!(
            "Git clone failed: {}",
            stderr.trim()
        )));
    }

    let src = match sub_path {
        Some(p) => tmp_dir.join(p),
        None => tmp_dir.clone(),
    };

    // Remove old, copy new
    let dest = PathBuf::from(repo_path);
    let _ = tokio::fs::remove_dir_all(&dest).await;
    if let Err(e) = copy_dir_recursive(&src, &dest).await {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        return Err(AppError::Internal(format!(
            "Failed to copy updated skill: {}",
            e
        )));
    }

    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    // Re-read SKILL.md
    let (name, description) = parse_skill_md(&dest.join("SKILL.md")).await;

    let updated = sqlx::query_as::<_, Skill>(
        r#"UPDATE skills SET name = $1, description = $2, updated_at = NOW()
           WHERE id = $3
           RETURNING id, name, description, instructions, git_url, repo_path,
                     visibility, enabled, tenant_id, user_id, created_by, created_at, updated_at"#,
    )
    .bind(&name)
    .bind(&description)
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(updated)
}

// ─── Delete ─────────────────────────────────────────────────────────────────

/// Remove a skill from the DB and delete its directory.
pub async fn delete(pool: &PgPool, auth_user: &AuthUser, id: Uuid) -> AppResult<()> {
    let skill = get_skill_with_access(pool, id, auth_user).await?;

    sqlx::query("DELETE FROM skills WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if let Some(ref path) = skill.repo_path
        && let Err(e) = tokio::fs::remove_dir_all(path).await
    {
        tracing::warn!("Failed to remove skill directory {}: {}", path, e);
    }

    Ok(())
}

// ─── Private helpers ────────────────────────────────────────────────────────

async fn get_skill_with_access(
    pool: &PgPool,
    id: Uuid,
    auth_user: &AuthUser,
) -> Result<Skill, AppError> {
    let skill = sqlx::query_as::<_, Skill>(
        r#"SELECT id, name, description, instructions, git_url, repo_path,
                  visibility, enabled, tenant_id, user_id, created_by, created_at, updated_at
           FROM skills WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Skill not found".to_string()))?;

    let has_access = auth_user.is_super_admin()
        || skill.user_id == Some(auth_user.user_id)
        || (skill.visibility == "public" && skill.tenant_id == auth_user.tenant_id);

    if !has_access {
        return Err(AppError::Forbidden("No access to this skill".to_string()));
    }

    Ok(skill)
}

// ─── Public helpers ─────────────────────────────────────────────────────────

/// Parse SKILL.md frontmatter for name + description.
pub async fn parse_skill_md(path: &Path) -> (String, Option<String>) {
    let fallback_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|f| f.to_str())
        .unwrap_or("unknown")
        .to_string();

    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(_) => return (fallback_name, None),
    };

    // Try YAML frontmatter first: ---\nname: xxx\ndescription: xxx\n---
    if content.starts_with("---") {
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() >= 3 {
            let frontmatter = parts[1];
            let mut name = None;
            let mut desc = None;
            for line in frontmatter.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("name:") {
                    name = Some(val.trim().to_string());
                } else if let Some(val) = line.strip_prefix("description:") {
                    desc = Some(val.trim().to_string());
                }
            }
            if let Some(n) = name {
                return (n, desc);
            }
        }
    }

    // Fallback: extract from markdown headings
    let name = content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").trim().to_string())
        .unwrap_or(fallback_name);

    let description = content
        .lines()
        .skip_while(|l| l.trim().is_empty() || l.starts_with('#') || l.starts_with("---"))
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string());

    (name, description)
}

/// Recursively copy a directory, skipping `.git`.
pub async fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dest).await?;
    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        // Skip .git directory
        if src_path
            .file_name()
            .map(|n| n == ".git")
            .unwrap_or(false)
        {
            continue;
        }

        if src_path.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dest_path)).await?;
        } else {
            tokio::fs::copy(&src_path, &dest_path).await?;
        }
    }
    Ok(())
}
