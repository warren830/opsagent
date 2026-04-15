use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use regex::Regex;
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

#[derive(Debug, Serialize)]
pub struct JiraSearchResult {
    pub key: String,
    pub summary: String,
    pub description_text: String,
    pub labels: Vec<String>,
    pub updated: String,
    pub url: String,
    pub comments: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfluencePage {
    pub id: String,
    pub title: String,
    pub body_markdown: String,
    pub updated: String,
    pub url: String,
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

    /// Search Jira issues using JQL.
    pub async fn search_issues(&self, jql: &str, max_results: usize) -> AppResult<Vec<JiraSearchResult>> {
        let url = format!(
            "{}/rest/api/3/search?jql={}&maxResults={}&fields=summary,description,status,labels,updated,comment",
            self.base_url,
            urlencoding::encode(jql),
            max_results
        );

        let resp = self
            .http
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira search: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!(
                "Jira search failed ({status}): {body}"
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::HttpClient(format!("Jira search parse: {e}")))?;

        let issues = body["issues"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let results = issues
            .iter()
            .map(|issue| {
                let key = issue["key"].as_str().unwrap_or_default().to_string();
                let fields = &issue["fields"];

                let summary = fields["summary"].as_str().unwrap_or_default().to_string();

                // Extract plain text from ADF description
                let description_text = fields
                    .get("description")
                    .filter(|d| !d.is_null())
                    .map(|d| Self::adf_to_plain_text(d))
                    .unwrap_or_default();

                let labels: Vec<String> = fields["labels"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                let updated = fields["updated"].as_str().unwrap_or_default().to_string();

                let url = format!("{}/browse/{}", self.base_url, key);

                // Extract comments
                let comments: Vec<String> = fields["comment"]["comments"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| {
                                c.get("body").map(|body| Self::adf_to_plain_text(body))
                            })
                            .filter(|text| !text.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();

                JiraSearchResult {
                    key,
                    summary,
                    description_text,
                    labels,
                    updated,
                    url,
                    comments,
                }
            })
            .collect();

        Ok(results)
    }

    /// Get Confluence pages in a space.
    pub async fn get_confluence_pages(&self, space_key: &str, limit: usize) -> AppResult<Vec<ConfluencePage>> {
        let url = format!(
            "{}/wiki/rest/api/content?spaceKey={}&limit={}&expand=body.storage,version",
            self.base_url,
            urlencoding::encode(space_key),
            limit
        );

        let resp = self
            .http
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Confluence get pages: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!(
                "Confluence get pages failed ({status}): {body}"
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::HttpClient(format!("Confluence parse pages: {e}")))?;

        let results = body["results"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let pages = results
            .iter()
            .map(|page| {
                let id = page["id"].as_str().unwrap_or_default().to_string();
                let title = page["title"].as_str().unwrap_or_default().to_string();

                let body_html = page["body"]["storage"]["value"]
                    .as_str()
                    .unwrap_or_default();
                let body_markdown = Self::html_to_markdown(body_html);

                let updated = page["version"]["when"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();

                let url = format!(
                    "{}/wiki{}",
                    self.base_url,
                    page["_links"]["webui"].as_str().unwrap_or_default()
                );

                ConfluencePage {
                    id,
                    title,
                    body_markdown,
                    updated,
                    url,
                }
            })
            .collect();

        Ok(pages)
    }

    /// Get a single Confluence page by ID.
    pub async fn get_confluence_page(&self, page_id: &str) -> AppResult<ConfluencePage> {
        let url = format!(
            "{}/wiki/rest/api/content/{}?expand=body.storage,version",
            self.base_url, page_id
        );

        let resp = self
            .http
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .map_err(|e| AppError::HttpClient(format!("Confluence get page: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::HttpClient(format!(
                "Confluence get page failed ({status}): {body}"
            )));
        }

        let page: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::HttpClient(format!("Confluence parse page: {e}")))?;

        let id = page["id"].as_str().unwrap_or_default().to_string();
        let title = page["title"].as_str().unwrap_or_default().to_string();

        let body_html = page["body"]["storage"]["value"]
            .as_str()
            .unwrap_or_default();
        let body_markdown = Self::html_to_markdown(body_html);

        let updated = page["version"]["when"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let url = format!(
            "{}/wiki{}",
            self.base_url,
            page["_links"]["webui"].as_str().unwrap_or_default()
        );

        Ok(ConfluencePage {
            id,
            title,
            body_markdown,
            updated,
            url,
        })
    }

    // ─── Helpers ────────────────────────────────────────────────────────────

    /// Convert Atlassian Document Format (ADF) JSON to plain text.
    /// Recursively walks the ADF tree and extracts text nodes.
    fn adf_to_plain_text(adf: &serde_json::Value) -> String {
        let mut parts: Vec<String> = Vec::new();
        Self::extract_adf_text(adf, &mut parts);
        parts.join(" ").trim().to_string()
    }

    fn extract_adf_text(node: &serde_json::Value, parts: &mut Vec<String>) {
        // If this is a text node, grab the text
        if node["type"].as_str() == Some("text") {
            if let Some(text) = node["text"].as_str() {
                parts.push(text.to_string());
            }
            return;
        }

        // Recurse into content array
        if let Some(content) = node["content"].as_array() {
            for child in content {
                Self::extract_adf_text(child, parts);
            }
        }
    }

    /// Convert HTML (Confluence storage format) to simple Markdown.
    /// Handles common elements: headings, paragraphs, code, lists, links, emphasis.
    pub fn html_to_markdown(html: &str) -> String {
        let mut text = html.to_string();

        // Replace <br> / <br/> with newlines
        let re_br = Regex::new(r"(?i)<br\s*/?>").unwrap();
        text = re_br.replace_all(&text, "\n").to_string();

        // Headings h1-h6
        for level in 1..=6 {
            let prefix = "#".repeat(level);
            let re_h = Regex::new(&format!(r"(?is)<h{level}[^>]*>(.*?)</h{level}>")).unwrap();
            text = re_h
                .replace_all(&text, |caps: &regex::Captures| {
                    format!("\n{} {}\n", prefix, caps[1].trim())
                })
                .to_string();
        }

        // Paragraphs to double newline
        let re_p_open = Regex::new(r"(?i)<p[^>]*>").unwrap();
        let re_p_close = Regex::new(r"(?i)</p>").unwrap();
        text = re_p_open.replace_all(&text, "\n").to_string();
        text = re_p_close.replace_all(&text, "\n").to_string();

        // Code blocks: <pre><code>...</code></pre> or <ac:structured-macro ac:name="code">
        let re_pre = Regex::new(r"(?is)<pre[^>]*>(.*?)</pre>").unwrap();
        text = re_pre
            .replace_all(&text, |caps: &regex::Captures| {
                format!("\n```\n{}\n```\n", caps[1].trim())
            })
            .to_string();

        // Inline code
        let re_code = Regex::new(r"(?is)<code[^>]*>(.*?)</code>").unwrap();
        text = re_code
            .replace_all(&text, |caps: &regex::Captures| {
                format!("`{}`", &caps[1])
            })
            .to_string();

        // Bold / strong
        let re_strong = Regex::new(r"(?is)<(?:strong|b)[^>]*>(.*?)</(?:strong|b)>").unwrap();
        text = re_strong
            .replace_all(&text, |caps: &regex::Captures| {
                format!("**{}**", &caps[1])
            })
            .to_string();

        // Italic / em
        let re_em = Regex::new(r"(?is)<(?:em|i)[^>]*>(.*?)</(?:em|i)>").unwrap();
        text = re_em
            .replace_all(&text, |caps: &regex::Captures| {
                format!("*{}*", &caps[1])
            })
            .to_string();

        // Links: <a href="...">text</a>
        let re_a = Regex::new(r#"(?is)<a[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#).unwrap();
        text = re_a
            .replace_all(&text, |caps: &regex::Captures| {
                format!("[{}]({})", &caps[2], &caps[1])
            })
            .to_string();

        // Unordered list items
        let re_li = Regex::new(r"(?is)<li[^>]*>(.*?)</li>").unwrap();
        text = re_li
            .replace_all(&text, |caps: &regex::Captures| {
                format!("- {}\n", caps[1].trim())
            })
            .to_string();

        // Strip <ul>, <ol>, <div>, and other remaining tags
        let re_tags = Regex::new(r"<[^>]+>").unwrap();
        text = re_tags.replace_all(&text, "").to_string();

        // Decode common HTML entities
        text = text.replace("&amp;", "&");
        text = text.replace("&lt;", "<");
        text = text.replace("&gt;", ">");
        text = text.replace("&quot;", "\"");
        text = text.replace("&#39;", "'");
        text = text.replace("&nbsp;", " ");

        // Collapse multiple blank lines into at most two newlines
        let re_blank = Regex::new(r"\n{3,}").unwrap();
        text = re_blank.replace_all(&text, "\n\n").to_string();

        text.trim().to_string()
    }
}
