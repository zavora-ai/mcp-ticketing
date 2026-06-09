//! MCP tool surface for the ticketing platform.
//!
//! Reads (tickets, comments, SLA status/report, workload) are `read_only`. Most
//! writes are `internal_write`. Two have external-facing weight and are gated
//! (`requires_approval`): `send_public_reply` (a reply visible to the requester —
//! `external_write`) and `close_ticket` (terminal disposition of a case).

use crate::store::TicketingStore;
use crate::types::*;
use adk_mcp_sdk::{HealthCheck, HealthStatus};
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use std::sync::Arc;

fn dactor() -> String { "agent".into() }
fn dlimit() -> usize { 50 }

// ─── inputs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateQueueInput { pub name: String, #[serde(default = "dgen")] pub category: String, #[serde(default = "dactor")] pub actor: String }
fn dgen() -> String { "general".into() }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueueIdInput { pub queue_id: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EmptyInput {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTicketInput {
    pub queue_id: String,
    pub subject: String,
    #[serde(default)] pub description: String,
    pub requester: String,
    #[serde(default = "dnormal")] pub priority: Priority,
    #[serde(default)] pub tags: Vec<String>,
    #[serde(default = "dactor")] pub actor: String,
}
fn dnormal() -> Priority { Priority::Normal }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TicketIdInput { pub ticket_id: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTicketsInput { pub queue_id: Option<String>, pub status: Option<Status>, pub assignee: Option<String> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssignInput { pub ticket_id: String, pub assignee: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetPriorityInput { pub ticket_id: String, pub priority: Priority, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetStatusInput { pub ticket_id: String, pub status: Status, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CloseInput { pub ticket_id: String, #[serde(default)] pub resolution: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TagInput { pub ticket_id: String, pub tag: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EscalateInput { pub ticket_id: String, #[serde(default)] pub reason: String, #[serde(default = "dactor")] pub actor: String }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommentInput { pub ticket_id: String, #[serde(default = "dactor")] pub author: String, pub body: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommentsForInput { pub ticket_id: String, #[serde(default)] pub include_internal: bool }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SlaReportInput { pub queue_id: Option<String> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AuditLogInput { #[serde(default = "dlimit")] pub limit: usize }

// ─── server ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TicketingServer { pub store: Arc<TicketingStore> }

#[tool_router(server_handler)]
impl TicketingServer {
    // queues
    #[tool(description = "Create a queue with the default SLA policy (per-priority response/resolution targets).")]
    fn create_queue(&self, Parameters(i): Parameters<CreateQueueInput>) -> String {
        let q = self.store.create_queue(&i.name, &i.category, SlaPolicy::default(), &i.actor);
        serde_json::to_string_pretty(&q).unwrap()
    }

    #[tool(description = "Get a queue and its SLA policy.")]
    fn get_queue(&self, Parameters(i): Parameters<QueueIdInput>) -> String {
        match self.store.get_queue(&i.queue_id) {
            Some(q) => serde_json::to_string_pretty(&q).unwrap(), None => format!("Queue not found: {}", i.queue_id) }
    }

    #[tool(description = "List all queues.")]
    fn list_queues(&self, Parameters(_): Parameters<EmptyInput>) -> String {
        let v = self.store.list_queues();
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "queues": v})).unwrap()
    }

    // tickets
    #[tool(description = "Create (intake) a ticket. SLA response/resolution deadlines are auto-computed from the queue policy + priority.")]
    fn create_ticket(&self, Parameters(i): Parameters<CreateTicketInput>) -> String {
        match self.store.create_ticket(&i.queue_id, &i.subject, &i.description, &i.requester, i.priority, i.tags, &i.actor) {
            Ok(t) => serde_json::to_string_pretty(&t).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Get a ticket by id.")]
    fn get_ticket(&self, Parameters(i): Parameters<TicketIdInput>) -> String {
        match self.store.get_ticket(&i.ticket_id) {
            Some(t) => serde_json::to_string_pretty(&t).unwrap(), None => format!("Ticket not found: {}", i.ticket_id) }
    }

    #[tool(description = "List tickets, optionally by queue, status, and/or assignee.")]
    fn list_tickets(&self, Parameters(i): Parameters<ListTicketsInput>) -> String {
        let v = self.store.list_tickets(i.queue_id.as_deref(), i.status, i.assignee.as_deref());
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "tickets": v})).unwrap()
    }

    #[tool(description = "Assign a ticket to an agent (moves New -> Open).")]
    fn assign_ticket(&self, Parameters(i): Parameters<AssignInput>) -> String {
        match self.store.assign_ticket(&i.ticket_id, &i.assignee, &i.actor) {
            Ok(t) => serde_json::to_string_pretty(&t).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Set a ticket's priority (recomputes SLA deadlines).")]
    fn set_priority(&self, Parameters(i): Parameters<SetPriorityInput>) -> String {
        match self.store.set_priority(&i.ticket_id, i.priority, &i.actor) {
            Ok(t) => serde_json::to_string_pretty(&t).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Transition a ticket's status (new/open/pending/resolved). Use close_ticket to close.")]
    fn set_status(&self, Parameters(i): Parameters<SetStatusInput>) -> String {
        match self.store.set_status(&i.ticket_id, i.status, &i.actor) {
            Ok(t) => serde_json::to_string_pretty(&t).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Close a ticket with a resolution (terminal). Gated.")]
    fn close_ticket(&self, Parameters(i): Parameters<CloseInput>) -> String {
        match self.store.close_ticket(&i.ticket_id, &i.resolution, &i.actor) {
            Ok(t) => serde_json::to_string_pretty(&t).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Add a tag to a ticket.")]
    fn add_tag(&self, Parameters(i): Parameters<TagInput>) -> String {
        match self.store.add_tag(&i.ticket_id, &i.tag, &i.actor) {
            Ok(t) => serde_json::to_string_pretty(&t).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Escalate a ticket: bumps escalation level, raises priority a notch, and re-baselines SLA.")]
    fn escalate_ticket(&self, Parameters(i): Parameters<EscalateInput>) -> String {
        match self.store.escalate_ticket(&i.ticket_id, &i.reason, &i.actor) {
            Ok(t) => serde_json::to_string_pretty(&t).unwrap(), Err(e) => format!("Error: {e}") }
    }

    // comments
    #[tool(description = "Add an INTERNAL staff-only note to a ticket (not visible to the requester).")]
    fn add_internal_note(&self, Parameters(i): Parameters<CommentInput>) -> String {
        match self.store.add_comment(&i.ticket_id, &i.author, &i.body, false, &i.actor) {
            Ok(c) => serde_json::to_string_pretty(&c).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Send a PUBLIC reply to the requester (counts as first response, sets SLA response time). External-facing — gated.")]
    fn send_public_reply(&self, Parameters(i): Parameters<CommentInput>) -> String {
        match self.store.add_comment(&i.ticket_id, &i.author, &i.body, true, &i.actor) {
            Ok(c) => serde_json::to_string_pretty(&c).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "List a ticket's comments (set include_internal=true for staff notes).")]
    fn list_comments(&self, Parameters(i): Parameters<CommentsForInput>) -> String {
        let v = self.store.comments_for(&i.ticket_id, i.include_internal);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "comments": v})).unwrap()
    }

    // SLA / analytics
    #[tool(description = "SLA status for one ticket: response/resolution breach flags and minutes remaining.")]
    fn sla_status(&self, Parameters(i): Parameters<TicketIdInput>) -> String {
        match self.store.sla_status(&i.ticket_id) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Ticket not found: {}", i.ticket_id) }
    }

    #[tool(description = "SLA report across a queue (or all): open, response/resolution breaches, and at-risk-next-2h counts.")]
    fn sla_report(&self, Parameters(i): Parameters<SlaReportInput>) -> String {
        serde_json::to_string_pretty(&self.store.sla_report(i.queue_id.as_deref())).unwrap()
    }

    #[tool(description = "Workload: open ticket counts per assignee (and unassigned).")]
    fn workload(&self, Parameters(_): Parameters<EmptyInput>) -> String {
        serde_json::to_string_pretty(&self.store.workload()).unwrap()
    }

    #[tool(description = "Recent audit-trail entries (most recent first).")]
    fn audit_log(&self, Parameters(i): Parameters<AuditLogInput>) -> String {
        let v = self.store.audit_log(i.limit);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "entries": v})).unwrap()
    }
}

#[async_trait::async_trait]
impl HealthCheck for TicketingServer {
    async fn check_health(&self) -> HealthStatus {
        HealthStatus { healthy: true, message: Some("operational".into()), latency_ms: Some(1) }
    }
}
