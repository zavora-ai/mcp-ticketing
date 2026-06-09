//! Service-desk / ticketing platform domain model.
//!
//! Broad case-ticketing platform: queues with SLA policies, tickets with
//! priority and computed SLA due dates + breach detection, intake & triage,
//! assignment, a status workflow, public/internal comments, escalation, tags,
//! and SLA/workload analytics. The named agents (Student Support, Matter Intake,
//! Privacy Request) are clients of this platform.

use chrono::{DateTime, Utc};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

// ─── queues & SLA policy ─────────────────────────────────────────────────────

/// Per-priority SLA targets (in hours) for first response and resolution.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SlaPolicy {
    pub response_hours_urgent: f64,
    pub resolution_hours_urgent: f64,
    pub response_hours_high: f64,
    pub resolution_hours_high: f64,
    pub response_hours_normal: f64,
    pub resolution_hours_normal: f64,
    pub response_hours_low: f64,
    pub resolution_hours_low: f64,
}

impl Default for SlaPolicy {
    fn default() -> Self {
        SlaPolicy {
            response_hours_urgent: 1.0, resolution_hours_urgent: 8.0,
            response_hours_high: 4.0, resolution_hours_high: 24.0,
            response_hours_normal: 8.0, resolution_hours_normal: 72.0,
            response_hours_low: 24.0, resolution_hours_low: 168.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Queue {
    pub id: String,
    pub name: String,
    /// Domain/category, e.g. "student-support", "legal-intake", "privacy".
    pub category: String,
    pub sla: SlaPolicy,
    pub created_at: DateTime<Utc>,
}

// ─── tickets ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Newly created, not yet triaged.
    New,
    /// Triaged/assigned, being worked.
    Open,
    /// Awaiting requester response.
    Pending,
    /// Work done, awaiting confirmation.
    Resolved,
    /// Terminal.
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Ticket {
    pub id: String,
    pub queue_id: String,
    pub subject: String,
    pub description: String,
    pub requester: String,
    pub priority: Priority,
    pub status: Status,
    pub assignee: Option<String>,
    pub tags: Vec<String>,
    /// Escalation tier (0 = none).
    pub escalation_level: u8,
    /// Computed SLA deadlines.
    pub response_due: DateTime<Utc>,
    pub resolution_due: DateTime<Utc>,
    /// First-response / resolution timestamps (set when they happen).
    pub first_response_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── comments ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Comment {
    pub id: String,
    pub ticket_id: String,
    pub author: String,
    pub body: String,
    /// Public comments are visible to the requester (count as a reply); internal
    /// notes are staff-only.
    pub public: bool,
    pub at: DateTime<Utc>,
}

// ─── audit trail ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuditEntry {
    pub at: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub detail: String,
}
