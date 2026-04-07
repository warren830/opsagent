use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Scan {
    pub id: Uuid,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub regions: Vec<String>,
    pub services: Vec<String>,
    pub status: String,
    pub finding_count: i32,
    pub summary: serde_json::Value,
    pub error_message: Option<String>,
    pub report_path: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Finding {
    pub id: Uuid,
    pub scan_id: Uuid,
    pub service: String,
    pub check_id: String,
    pub severity: String,
    pub category: String,
    pub short_desc: String,
    pub description: Option<String>,
    pub resource_arn: Option<String>,
    pub resource_name: Option<String>,
    pub region: Option<String>,
    pub account_id: Option<String>,
    pub compliant: bool,
    pub remediation: Option<String>,
    pub detail: serde_json::Value,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ScanRequest {
    pub account_id: Option<String>,
    pub regions: Option<Vec<String>>,
    pub services: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct FindingListQuery {
    pub scan_id: Option<Uuid>,
    pub severity: Option<String>,
    pub category: Option<String>,
    pub service: Option<String>,
    pub region: Option<String>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_findings: i64,
    pub by_severity: SeverityCounts,
    pub by_category: CategoryCounts,
    pub by_service: Vec<ServiceCount>,
    pub last_scan: Option<Scan>,
}

#[derive(Debug, Serialize)]
pub struct SeverityCounts {
    pub high: i64,
    pub medium: i64,
    pub low: i64,
    pub info: i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryCounts {
    pub security: i64,
    pub cost: i64,
    pub reliability: i64,
    pub performance: i64,
    pub operations: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ServiceCount {
    pub service: String,
    pub count: i64,
}
