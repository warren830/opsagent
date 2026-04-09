use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct JiraClient {
    http: reqwest::Client,
    pub base_url: String,
    auth_header: String,
    pub project_key: String,
    pub default_issue_type: String,
    pub default_labels: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JiraIssue {
    pub id: String,
    pub key: String,
    #[serde(rename = "self")]
    pub self_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateIssuePayload {
    fields: CreateIssueFields,
}

#[derive(Debug, Serialize)]
struct CreateIssueFields {
    project: ProjectRef,
    summary: String,
    description: serde_json::Value,
    issuetype: IssueTypeRef,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    labels: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProjectRef {
    key: String,
}

#[derive(Debug, Serialize)]
struct IssueTypeRef {
    name: String,
}

#[derive(Debug, Serialize)]
struct TransitionPayload {
    transition: TransitionRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct TransitionRef {
    id: String,
}

#[derive(Debug, Serialize)]
struct CommentPayload {
    body: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct TransitionsResponse {
    transitions: Vec<TransitionEntry>,
}

#[derive(Debug, Deserialize)]
struct TransitionEntry {
    id: String,
    name: String,
}

// ─── Implementation ─────────────────────────────────────────────────────────

impl JiraClient {
    /// Create a JiraClient from channel credentials JSONB.
    pub fn from_credentials(creds: &serde_json::Value) -> AppResult<Self> {
        let base_url = creds["base_url"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("Jira base_url missing".into()))?
            .trim_end_matches('/')
            .to_string();

        let email = creds["email"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("Jira email missing".into()))?;

        let api_token = creds["api_token"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("Jira api_token missing".into()))?;

        let project_key = creds["project_key"].as_str().unwrap_or("OPS").to_string();

        let default_issue_type = creds["default_issue_type"].as_str().unwrap_or("Task").to_string();

        let default_labels: Vec<String> = creds["default_labels"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        // Basic auth: base64(email:api_token)
        let auth = BASE64.encode(format!("{email}:{api_token}"));

        Ok(Self {
            http: reqwest::Client::new(),
            base_url,
            auth_header: format!("Basic {auth}"),
            project_key,
            default_issue_type,
            default_labels,
        })
    }

    /// Create a Jira issue.
    pub async fn create_issue(
        &self,
        summary: &str,
        description: &str,
        issue_type: Option<&str>,
        labels: Option<Vec<String>>,
    ) -> AppResult<JiraIssue> {
        let issue_type_name = issue_type.unwrap_or(&self.default_issue_type).to_string();
        let mut all_labels = self.default_labels.clone();
        if let Some(extra) = labels {
            all_labels.extend(extra);
        }

        // Atlassian Document Format (ADF) for description
        let desc_adf = serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": description
                }]
            }]
        });

        let payload = CreateIssuePayload {
            fields: CreateIssueFields {
                project: ProjectRef {
                    key: self.project_key.clone(),
                },
                summary: summary.to_string(),
                description: desc_adf,
                issuetype: IssueTypeRef { name: issue_type_name },
                labels: all_labels,
            },
        };

        let resp = self
            .http
            .post(format!("{}/rest/api/3/issue", self.base_url))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira create issue: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!(
                "Jira create issue failed ({status}): {body}"
            )));
        }

        resp.json::<JiraIssue>()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira parse response: {e}")))
    }

    /// Transition an issue to a target status (e.g. "Done", "In Progress").
    /// Jira transitions are ID-based, so we first fetch available transitions
    /// and match by name.
    pub async fn transition_issue(&self, issue_key: &str, target_status: &str, comment: Option<&str>) -> AppResult<()> {
        // 1. Get available transitions
        let resp = self
            .http
            .get(format!("{}/rest/api/3/issue/{issue_key}/transitions", self.base_url))
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira get transitions: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!("Jira get transitions failed: {body}")));
        }

        let transitions: TransitionsResponse = resp
            .json()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira parse transitions: {e}")))?;

        // 2. Find matching transition (case-insensitive)
        let target_lower = target_status.to_lowercase();
        let transition = transitions
            .transitions
            .iter()
            .find(|t| t.name.to_lowercase() == target_lower)
            .ok_or_else(|| {
                let available: Vec<&str> = transitions.transitions.iter().map(|t| t.name.as_str()).collect();
                AppError::BadRequest(format!(
                    "No transition to '{target_status}' available. Available: {:?}",
                    available
                ))
            })?;

        // 3. Execute transition
        let update = comment.map(|c| {
            serde_json::json!({
                "comment": [{
                    "add": {
                        "body": {
                            "type": "doc",
                            "version": 1,
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": c }]
                            }]
                        }
                    }
                }]
            })
        });

        let payload = TransitionPayload {
            transition: TransitionRef {
                id: transition.id.clone(),
            },
            update,
        };

        let resp = self
            .http
            .post(format!("{}/rest/api/3/issue/{issue_key}/transitions", self.base_url))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira transition: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!("Jira transition failed: {body}")));
        }

        // Also add comment separately if transition doesn't support inline comment
        if let Some(c) = comment {
            let _ = self.add_comment(issue_key, c).await;
        }

        Ok(())
    }

    /// Add a comment to an issue.
    pub async fn add_comment(&self, issue_key: &str, comment: &str) -> AppResult<()> {
        let payload = CommentPayload {
            body: serde_json::json!({
                "type": "doc",
                "version": 1,
                "content": [{
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": comment }]
                }]
            }),
        };

        let resp = self
            .http
            .post(format!("{}/rest/api/3/issue/{issue_key}/comment", self.base_url))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira add comment: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!("Jira add comment failed: {body}")));
        }

        Ok(())
    }

    /// Get issue details.
    pub async fn get_issue(&self, issue_key: &str) -> AppResult<serde_json::Value> {
        let resp = self
            .http
            .get(format!(
                "{}/rest/api/3/issue/{issue_key}?fields=summary,status,assignee,labels,created,updated",
                self.base_url
            ))
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira get issue: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!(
                "Jira get issue failed ({status}): {body}"
            )));
        }

        resp.json()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira parse issue: {e}")))
    }
}
