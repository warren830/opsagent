use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub tenants: i64,
    pub users: i64,
    pub skills: i64,
    pub clusters: i64,
    pub issues_open: i64,
    pub active_sessions: i64,
}
