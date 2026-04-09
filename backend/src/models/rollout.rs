use serde::{Deserialize, Serialize};

/// Summary of an Argo Rollout — parsed from K8s DynamicObject, not a DB model.
#[derive(Debug, Clone, Serialize)]
pub struct RolloutSummary {
    pub name: String,
    pub namespace: String,
    pub strategy: String, // "Canary" | "BlueGreen"
    pub status: String,   // "Healthy" | "Progressing" | "Degraded" | "Paused"
    pub desired_replicas: i32,
    pub ready_replicas: i32,
    pub updated_replicas: i32,
    pub canary_weight: Option<i32>,
    pub current_step: Option<i32>,
    pub total_steps: Option<i32>,
    pub active_service: Option<String>,
    pub preview_service: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Detailed view of an Argo Rollout with canary steps and container info.
#[derive(Debug, Clone, Serialize)]
pub struct RolloutDetail {
    pub summary: RolloutSummary,
    pub canary_steps: Vec<CanaryStep>,
    pub containers: Vec<ContainerInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CanaryStep {
    pub index: i32,
    pub action: String, // "setWeight" | "pause" | "analysis" | "setCanaryScale" | "experiment"
    pub value: serde_json::Value,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainerInfo {
    pub name: String,
    pub image: String,
}

/// Summary of an Argo AnalysisRun.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisRunSummary {
    pub name: String,
    pub phase: String, // "Successful" | "Failed" | "Running" | "Pending" | "Error"
    pub metrics: Vec<AnalysisMetric>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisMetric {
    pub name: String,
    pub phase: String,
    pub value: Option<String>,
    pub message: Option<String>,
}

/// Request body for promote.
#[derive(Debug, Deserialize)]
pub struct PromoteRequest {
    /// false = advance one step, true = promote to full rollout
    #[serde(default)]
    pub full: bool,
}

/// Input for a single canary step when changing strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanaryStepInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_weight: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause: Option<serde_json::Value>, // {} for indefinite, {"duration":"60s"} for timed
}

/// Request body for changing rollout strategy.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeStrategyRequest {
    pub strategy: String, // "canary" | "blueGreen" | "rollingUpdate"
    #[serde(default)]
    pub canary_steps: Option<Vec<CanaryStepInput>>,
    #[serde(default)]
    pub active_service: Option<String>,
    #[serde(default)]
    pub preview_service: Option<String>,
    #[serde(default)]
    pub auto_promotion_enabled: Option<bool>,
}

/// Parse a DynamicObject JSON into RolloutSummary.
pub fn parse_rollout_summary(obj: &serde_json::Value) -> Option<RolloutSummary> {
    let metadata = obj.get("metadata")?;
    let spec = obj.get("spec")?;
    let empty_obj = serde_json::json!({});
    let status = obj.get("status").unwrap_or(&empty_obj);

    let name = metadata.get("name")?.as_str()?.to_string();
    let namespace = metadata
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    // Determine strategy
    let strategy = if spec.get("strategy").and_then(|s| s.get("canary")).is_some() {
        "Canary".to_string()
    } else if spec.get("strategy").and_then(|s| s.get("blueGreen")).is_some() {
        "BlueGreen".to_string()
    } else {
        "Unknown".to_string()
    };

    // Parse phase from status
    let phase = status
        .get("phase")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let desired = spec.get("replicas").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let ready = status.get("readyReplicas").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let updated = status.get("updatedReplicas").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

    // Canary weight from status
    let canary_weight = status
        .pointer("/canary/weight")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);

    // Current step index
    let current_step = status
        .get("currentStepIndex")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);

    // Total steps from spec
    let total_steps = spec
        .pointer("/strategy/canary/steps")
        .and_then(|v| v.as_array())
        .map(|arr| arr.len() as i32);

    let created_at = metadata
        .get("creationTimestamp")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let updated_at = status.get("observedGeneration").map(|_| {
        metadata
            .get("creationTimestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    });

    // BlueGreen service names from spec
    let active_service = spec
        .pointer("/strategy/blueGreen/activeService")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let preview_service = spec
        .pointer("/strategy/blueGreen/previewService")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(RolloutSummary {
        name,
        namespace,
        strategy,
        status: phase,
        desired_replicas: desired,
        ready_replicas: ready,
        updated_replicas: updated,
        canary_weight,
        current_step,
        total_steps,
        active_service,
        preview_service,
        created_at,
        updated_at,
    })
}

/// Parse canary steps from a Rollout spec.
pub fn parse_canary_steps(obj: &serde_json::Value, current_step_index: Option<i32>) -> Vec<CanaryStep> {
    let steps = obj.pointer("/spec/strategy/canary/steps").and_then(|v| v.as_array());

    let Some(steps) = steps else {
        return Vec::new();
    };

    let current = current_step_index.unwrap_or(-1);

    steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let (action, value) = if let Some(w) = step.get("setWeight") {
                ("setWeight".to_string(), w.clone())
            } else if step.get("pause").is_some() {
                let duration = step
                    .pointer("/pause/duration")
                    .cloned()
                    .unwrap_or(serde_json::json!("indefinite"));
                ("pause".to_string(), duration)
            } else if step.get("analysis").is_some() {
                (
                    "analysis".to_string(),
                    step.get("analysis").cloned().unwrap_or_default(),
                )
            } else if let Some(s) = step.get("setCanaryScale") {
                ("setCanaryScale".to_string(), s.clone())
            } else if step.get("experiment").is_some() {
                (
                    "experiment".to_string(),
                    step.get("experiment").cloned().unwrap_or_default(),
                )
            } else {
                ("unknown".to_string(), serde_json::json!({}))
            };

            CanaryStep {
                index: i as i32,
                action,
                value,
                completed: (i as i32) < current,
            }
        })
        .collect()
}

/// Parse containers from a Rollout spec.
pub fn parse_containers(obj: &serde_json::Value) -> Vec<ContainerInfo> {
    obj.pointer("/spec/template/spec/containers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    Some(ContainerInfo {
                        name: c.get("name")?.as_str()?.to_string(),
                        image: c.get("image")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse an AnalysisRun DynamicObject into AnalysisRunSummary.
pub fn parse_analysis_run(obj: &serde_json::Value) -> Option<AnalysisRunSummary> {
    let metadata = obj.get("metadata")?;
    let empty_obj = serde_json::json!({});
    let status = obj.get("status").unwrap_or(&empty_obj);

    let name = metadata.get("name")?.as_str()?.to_string();
    let phase = status
        .get("phase")
        .and_then(|v| v.as_str())
        .unwrap_or("Pending")
        .to_string();

    let metrics = status
        .get("metricResults")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    Some(AnalysisMetric {
                        name: m.get("name")?.as_str()?.to_string(),
                        phase: m.get("phase").and_then(|v| v.as_str()).unwrap_or("Pending").to_string(),
                        value: m.get("value").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        message: m.get("message").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let created_at = metadata
        .get("creationTimestamp")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(AnalysisRunSummary {
        name,
        phase,
        metrics,
        created_at,
    })
}
