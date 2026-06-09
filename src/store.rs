//! In-memory ticketing store with seeded data and engines.
//!
//! Thread-safe via per-collection `Mutex`. IDs come from a monotonic sequence
//! (`PREFIX-{n}` from 1000). Every state change appends to an audit trail.
//! Engines: SLA due-date computation + breach detection, status workflow,
//! assignment, escalation, public-reply first-response tracking, and
//! SLA/workload analytics.

use crate::types::*;
use chrono::{Duration, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct TicketingStore {
    queues: Mutex<HashMap<String, Queue>>,
    tickets: Mutex<HashMap<String, Ticket>>,
    comments: Mutex<Vec<Comment>>,
    audit_log: Mutex<Vec<AuditEntry>>,
    seq: Mutex<u64>,
}

impl Default for TicketingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TicketingStore {
    pub fn new() -> Self {
        let s = TicketingStore {
            queues: Mutex::new(HashMap::new()),
            tickets: Mutex::new(HashMap::new()),
            comments: Mutex::new(Vec::new()),
            audit_log: Mutex::new(Vec::new()),
            seq: Mutex::new(1000),
        };
        s.seed();
        s
    }

    fn next(&self, prefix: &str) -> String {
        let mut n = self.seq.lock().unwrap();
        *n += 1;
        format!("{prefix}-{n}")
    }

    fn audit(&self, actor: &str, action: &str, detail: impl Into<String>) {
        self.audit_log.lock().unwrap().push(AuditEntry { at: Utc::now(), actor: actor.to_string(), action: action.to_string(), detail: detail.into() });
    }

    pub fn queue_exists(&self, id: &str) -> bool { self.queues.lock().unwrap().contains_key(id) }

    // ─── queues ──────────────────────────────────────────────────────────

    pub fn create_queue(&self, name: &str, category: &str, sla: SlaPolicy, actor: &str) -> Queue {
        let q = Queue { id: self.next("QUE"), name: name.to_string(), category: category.to_string(), sla, created_at: Utc::now() };
        self.queues.lock().unwrap().insert(q.id.clone(), q.clone());
        self.audit(actor, "create_queue", q.id.clone());
        q
    }

    pub fn get_queue(&self, id: &str) -> Option<Queue> {
        self.queues.lock().unwrap().get(id).cloned()
    }

    pub fn list_queues(&self) -> Vec<Queue> {
        let mut v: Vec<Queue> = self.queues.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    // ─── tickets ───────────────────────────────────────────────────────────

    /// Create (intake) a ticket. SLA response/resolution deadlines are computed
    /// from the queue's policy and the priority.
    pub fn create_ticket(&self, queue_id: &str, subject: &str, description: &str, requester: &str, priority: Priority, tags: Vec<String>, actor: &str) -> Result<Ticket, String> {
        let queue = self.get_queue(queue_id).ok_or_else(|| format!("Queue not found: {queue_id}"))?;
        let now = Utc::now();
        let (resp_h, reso_h) = sla_hours(&queue.sla, priority);
        let t = Ticket {
            id: self.next("TKT"),
            queue_id: queue_id.to_string(),
            subject: subject.to_string(),
            description: description.to_string(),
            requester: requester.to_string(),
            priority,
            status: Status::New,
            assignee: None,
            tags,
            escalation_level: 0,
            response_due: now + Duration::minutes((resp_h * 60.0) as i64),
            resolution_due: now + Duration::minutes((reso_h * 60.0) as i64),
            first_response_at: None,
            resolved_at: None,
            closed_at: None,
            created_at: now,
            updated_at: now,
        };
        self.tickets.lock().unwrap().insert(t.id.clone(), t.clone());
        self.audit(actor, "create_ticket", format!("{} {:?}", t.id, priority));
        Ok(t)
    }

    pub fn get_ticket(&self, id: &str) -> Option<Ticket> {
        self.tickets.lock().unwrap().get(id).cloned()
    }

    pub fn list_tickets(&self, queue_id: Option<&str>, status: Option<Status>, assignee: Option<&str>) -> Vec<Ticket> {
        let mut v: Vec<Ticket> = self.tickets.lock().unwrap().values()
            .filter(|t| queue_id.is_none_or(|q| t.queue_id == q))
            .filter(|t| status.is_none_or(|s| t.status == s))
            .filter(|t| assignee.is_none_or(|a| t.assignee.as_deref() == Some(a)))
            .cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v
    }

    /// Assign a ticket; moves New -> Open.
    pub fn assign_ticket(&self, ticket_id: &str, assignee: &str, actor: &str) -> Result<Ticket, String> {
        let mut tickets = self.tickets.lock().unwrap();
        let t = tickets.get_mut(ticket_id).ok_or_else(|| format!("Ticket not found: {ticket_id}"))?;
        if t.status == Status::Closed { return Err("cannot assign a closed ticket".into()); }
        t.assignee = Some(assignee.to_string());
        if t.status == Status::New { t.status = Status::Open; }
        t.updated_at = Utc::now();
        let out = t.clone();
        drop(tickets);
        self.audit(actor, "assign_ticket", format!("{ticket_id} -> {assignee}"));
        Ok(out)
    }

    pub fn set_priority(&self, ticket_id: &str, priority: Priority, actor: &str) -> Result<Ticket, String> {
        let mut tickets = self.tickets.lock().unwrap();
        let t = tickets.get_mut(ticket_id).ok_or_else(|| format!("Ticket not found: {ticket_id}"))?;
        t.priority = priority;
        // Recompute SLA deadlines from creation using the new priority.
        let queue = self.queues.lock().unwrap().get(&t.queue_id).cloned();
        if let Some(q) = queue {
            let (resp_h, reso_h) = sla_hours(&q.sla, priority);
            t.response_due = t.created_at + Duration::minutes((resp_h * 60.0) as i64);
            t.resolution_due = t.created_at + Duration::minutes((reso_h * 60.0) as i64);
        }
        t.updated_at = Utc::now();
        let out = t.clone();
        drop(tickets);
        self.audit(actor, "set_priority", format!("{ticket_id} -> {priority:?}"));
        Ok(out)
    }

    /// Transition status with validation. Closing is handled by `close_ticket`.
    pub fn set_status(&self, ticket_id: &str, status: Status, actor: &str) -> Result<Ticket, String> {
        let mut tickets = self.tickets.lock().unwrap();
        let t = tickets.get_mut(ticket_id).ok_or_else(|| format!("Ticket not found: {ticket_id}"))?;
        if t.status == Status::Closed { return Err("ticket is closed; reopen is not supported".into()); }
        let now = Utc::now();
        if status == Status::Resolved && t.resolved_at.is_none() { t.resolved_at = Some(now); }
        t.status = status;
        t.updated_at = now;
        let out = t.clone();
        drop(tickets);
        self.audit(actor, "set_status", format!("{ticket_id} -> {status:?}"));
        Ok(out)
    }

    /// Close a ticket (terminal). Gated at the tool layer.
    pub fn close_ticket(&self, ticket_id: &str, resolution: &str, actor: &str) -> Result<Ticket, String> {
        let mut tickets = self.tickets.lock().unwrap();
        let t = tickets.get_mut(ticket_id).ok_or_else(|| format!("Ticket not found: {ticket_id}"))?;
        if t.status == Status::Closed { return Err("ticket already closed".into()); }
        let now = Utc::now();
        if t.resolved_at.is_none() { t.resolved_at = Some(now); }
        t.status = Status::Closed;
        t.closed_at = Some(now);
        t.updated_at = now;
        let out = t.clone();
        drop(tickets);
        self.audit(actor, "close_ticket", format!("{ticket_id}: {resolution}"));
        Ok(out)
    }

    pub fn add_tag(&self, ticket_id: &str, tag: &str, actor: &str) -> Result<Ticket, String> {
        let mut tickets = self.tickets.lock().unwrap();
        let t = tickets.get_mut(ticket_id).ok_or_else(|| format!("Ticket not found: {ticket_id}"))?;
        let tag = tag.to_lowercase();
        if !t.tags.contains(&tag) { t.tags.push(tag.clone()); }
        t.updated_at = Utc::now();
        let out = t.clone();
        drop(tickets);
        self.audit(actor, "add_tag", format!("{ticket_id} #{tag}"));
        Ok(out)
    }

    /// Escalate a ticket: bumps escalation level and raises priority a notch.
    pub fn escalate_ticket(&self, ticket_id: &str, reason: &str, actor: &str) -> Result<Ticket, String> {
        let mut tickets = self.tickets.lock().unwrap();
        let t = tickets.get_mut(ticket_id).ok_or_else(|| format!("Ticket not found: {ticket_id}"))?;
        if t.status == Status::Closed { return Err("cannot escalate a closed ticket".into()); }
        t.escalation_level = t.escalation_level.saturating_add(1);
        t.priority = bump_priority(t.priority);
        // Recompute SLA from the new priority (re-baselined to now to reflect urgency).
        let queue = self.queues.lock().unwrap().get(&t.queue_id).cloned();
        if let Some(q) = queue {
            let (resp_h, reso_h) = sla_hours(&q.sla, t.priority);
            let now = Utc::now();
            t.response_due = now + Duration::minutes((resp_h * 60.0) as i64);
            t.resolution_due = now + Duration::minutes((reso_h * 60.0) as i64);
        }
        t.updated_at = Utc::now();
        let out = t.clone();
        drop(tickets);
        self.audit(actor, "escalate_ticket", format!("{ticket_id} L{} ({reason})", out.escalation_level));
        Ok(out)
    }

    // ─── comments ────────────────────────────────────────────────────────

    /// Add a comment. A public comment counts as a reply and sets first-response
    /// time if not already set. Public replies are gated at the tool layer.
    pub fn add_comment(&self, ticket_id: &str, author: &str, body: &str, public: bool, actor: &str) -> Result<Comment, String> {
        {
            let mut tickets = self.tickets.lock().unwrap();
            let t = tickets.get_mut(ticket_id).ok_or_else(|| format!("Ticket not found: {ticket_id}"))?;
            if public && t.first_response_at.is_none() {
                t.first_response_at = Some(Utc::now());
            }
            t.updated_at = Utc::now();
        }
        let c = Comment { id: self.next("CMT"), ticket_id: ticket_id.to_string(), author: author.to_string(), body: body.to_string(), public, at: Utc::now() };
        self.comments.lock().unwrap().push(c.clone());
        self.audit(actor, if public { "public_reply" } else { "internal_note" }, ticket_id.to_string());
        Ok(c)
    }

    pub fn comments_for(&self, ticket_id: &str, include_internal: bool) -> Vec<Comment> {
        let mut v: Vec<Comment> = self.comments.lock().unwrap().iter()
            .filter(|c| c.ticket_id == ticket_id && (include_internal || c.public))
            .cloned().collect();
        v.sort_by(|a, b| a.at.cmp(&b.at));
        v
    }

    // ─── SLA / analytics ─────────────────────────────────────────────────

    /// SLA status for one ticket: response/resolution breach flags and minutes
    /// remaining (negative if breached). Open tickets compare against now;
    /// resolved/closed compare against the actual timestamp.
    pub fn sla_status(&self, ticket_id: &str) -> Option<serde_json::Value> {
        let t = self.get_ticket(ticket_id)?;
        let now = Utc::now();
        // First response: breached if no response and past due, or responded late.
        let response_breached = match t.first_response_at {
            Some(at) => at > t.response_due,
            None => now > t.response_due && t.status != Status::Closed,
        };
        let resolution_ref = t.resolved_at.unwrap_or(now);
        let resolution_breached = match t.resolved_at {
            Some(at) => at > t.resolution_due,
            None => now > t.resolution_due && t.status != Status::Closed,
        };
        let resp_remaining = (t.response_due - t.first_response_at.unwrap_or(now)).num_minutes();
        let reso_remaining = (t.resolution_due - resolution_ref).num_minutes();
        Some(serde_json::json!({
            "ticket_id": t.id,
            "priority": t.priority,
            "status": t.status,
            "response_due": t.response_due,
            "resolution_due": t.resolution_due,
            "first_response_at": t.first_response_at,
            "resolved_at": t.resolved_at,
            "response_breached": response_breached,
            "resolution_breached": resolution_breached,
            "response_minutes_remaining": resp_remaining,
            "resolution_minutes_remaining": reso_remaining,
        }))
    }

    /// SLA report across a queue (or all): counts of open, breached response,
    /// breached resolution, and at-risk (due within 2h).
    pub fn sla_report(&self, queue_id: Option<&str>) -> serde_json::Value {
        let now = Utc::now();
        let tickets = self.tickets.lock().unwrap();
        let rel: Vec<&Ticket> = tickets.values().filter(|t| queue_id.is_none_or(|q| t.queue_id == q)).collect();
        let total = rel.len();
        let open = rel.iter().filter(|t| t.status != Status::Closed).count();
        let mut resp_breach = 0; let mut reso_breach = 0; let mut at_risk = 0;
        for t in &rel {
            let active = t.status != Status::Closed;
            if t.first_response_at.map(|a| a > t.response_due).unwrap_or(active && now > t.response_due) { resp_breach += 1; }
            if t.resolved_at.map(|a| a > t.resolution_due).unwrap_or(active && now > t.resolution_due) { reso_breach += 1; }
            if active && t.resolved_at.is_none() && now <= t.resolution_due && (t.resolution_due - now).num_minutes() <= 120 { at_risk += 1; }
        }
        serde_json::json!({
            "queue_id": queue_id,
            "total": total,
            "open": open,
            "response_breached": resp_breach,
            "resolution_breached": reso_breach,
            "at_risk_next_2h": at_risk,
        })
    }

    /// Workload by assignee: open ticket counts (unassigned bucketed as null).
    pub fn workload(&self) -> serde_json::Value {
        let tickets = self.tickets.lock().unwrap();
        let mut counts: HashMap<String, u64> = HashMap::new();
        for t in tickets.values().filter(|t| t.status != Status::Closed) {
            let key = t.assignee.clone().unwrap_or_else(|| "unassigned".into());
            *counts.entry(key).or_insert(0) += 1;
        }
        serde_json::json!({"open_by_assignee": counts})
    }

    pub fn audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        let log = self.audit_log.lock().unwrap();
        log.iter().rev().take(limit).cloned().collect()
    }

    // ─── seed ────────────────────────────────────────────────────────────

    fn seed(&self) {
        // Education: student support queue.
        let edu = self.create_queue("Student Support", "student-support", SlaPolicy::default(), "system");
        let t1 = self.create_ticket(&edu.id, "Cannot access course portal", "Student locked out of LMS", "student:stu-1001", Priority::High, vec!["access".into()], "system").unwrap();
        self.assign_ticket(&t1.id, "support.advisor", "system").ok();
        self.create_ticket(&edu.id, "Financial aid question", "Question about disbursement timing", "student:stu-1002", Priority::Normal, vec!["finaid".into()], "system").ok();

        // Legal: matter intake queue (slower SLA reflects legal review).
        let intake_sla = SlaPolicy {
            response_hours_urgent: 2.0, resolution_hours_urgent: 24.0,
            response_hours_high: 8.0, resolution_hours_high: 72.0,
            response_hours_normal: 24.0, resolution_hours_normal: 240.0,
            response_hours_low: 48.0, resolution_hours_low: 480.0,
        };
        let legal = self.create_queue("Legal Matter Intake", "legal-intake", intake_sla, "system");
        self.create_ticket(&legal.id, "New NDA review request", "Vendor NDA needs review before signing", "biz:procurement", Priority::Normal, vec!["nda".into(), "contract".into()], "system").ok();

        // Legal/Privacy: DSAR / privacy request queue (regulatory clock).
        let privacy_sla = SlaPolicy {
            response_hours_urgent: 24.0, resolution_hours_urgent: 720.0,   // ~30 days
            response_hours_high: 48.0, resolution_hours_high: 720.0,
            response_hours_normal: 72.0, resolution_hours_normal: 720.0,
            response_hours_low: 72.0, resolution_hours_low: 720.0,
        };
        let privacy = self.create_queue("Privacy Requests", "privacy", privacy_sla, "system");
        self.create_ticket(&privacy.id, "GDPR data access request", "Data subject requesting copy of personal data", "subject:jane.doe", Priority::High, vec!["dsar".into(), "gdpr".into()], "system").ok();
    }
}

// ─── SLA helpers ───────────────────────────────────────────────────────────

/// (response_hours, resolution_hours) for a priority under a policy.
fn sla_hours(p: &SlaPolicy, priority: Priority) -> (f64, f64) {
    match priority {
        Priority::Urgent => (p.response_hours_urgent, p.resolution_hours_urgent),
        Priority::High => (p.response_hours_high, p.resolution_hours_high),
        Priority::Normal => (p.response_hours_normal, p.resolution_hours_normal),
        Priority::Low => (p.response_hours_low, p.resolution_hours_low),
    }
}

fn bump_priority(p: Priority) -> Priority {
    match p {
        Priority::Low => Priority::Normal,
        Priority::Normal => Priority::High,
        Priority::High => Priority::Urgent,
        Priority::Urgent => Priority::Urgent,
    }
}
