//! Integration tests: intake + SLA computation, workflow, assignment, public
//! reply first-response, escalation, breach detection, and analytics.

use mcp_ticketing::store::TicketingStore;
use mcp_ticketing::types::*;

fn store() -> TicketingStore {
    TicketingStore::new()
}

fn first_queue(s: &TicketingStore, cat: &str) -> String {
    s.list_queues().into_iter().find(|q| q.category == cat).unwrap().id
}

#[test]
fn seed_loads() {
    let s = store();
    assert_eq!(s.list_queues().len(), 3);
    assert!(s.list_tickets(None, None, None).len() >= 4);
}

#[test]
fn intake_computes_sla_deadlines() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "Test", "body", "stu-x", Priority::Urgent, vec![], "agent").unwrap();
    // default urgent: response 1h, resolution 8h
    assert!(t.response_due > t.created_at);
    assert!((t.resolution_due - t.created_at).num_hours() == 8);
    assert_eq!(t.status, Status::New);
}

#[test]
fn assign_moves_new_to_open() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "T", "b", "r", Priority::Normal, vec![], "a").unwrap();
    let assigned = s.assign_ticket(&t.id, "advisor.jane", "a").unwrap();
    assert_eq!(assigned.status, Status::Open);
    assert_eq!(assigned.assignee.as_deref(), Some("advisor.jane"));
}

#[test]
fn public_reply_sets_first_response() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "T", "b", "r", Priority::Normal, vec![], "a").unwrap();
    assert!(t.first_response_at.is_none());
    s.add_comment(&t.id, "advisor", "Hi, looking into it", true, "a").unwrap();
    assert!(s.get_ticket(&t.id).unwrap().first_response_at.is_some());
    // internal note does NOT set first response
    let t2 = s.create_ticket(&q, "T2", "b", "r", Priority::Normal, vec![], "a").unwrap();
    s.add_comment(&t2.id, "advisor", "internal", false, "a").unwrap();
    assert!(s.get_ticket(&t2.id).unwrap().first_response_at.is_none());
}

#[test]
fn internal_notes_hidden_unless_requested() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "T", "b", "r", Priority::Normal, vec![], "a").unwrap();
    s.add_comment(&t.id, "advisor", "public hi", true, "a").unwrap();
    s.add_comment(&t.id, "advisor", "secret note", false, "a").unwrap();
    assert_eq!(s.comments_for(&t.id, false).len(), 1, "public only");
    assert_eq!(s.comments_for(&t.id, true).len(), 2, "all");
}

#[test]
fn escalation_raises_priority_and_level() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "T", "b", "r", Priority::Normal, vec![], "a").unwrap();
    let e = s.escalate_ticket(&t.id, "no response", "supervisor").unwrap();
    assert_eq!(e.escalation_level, 1);
    assert_eq!(e.priority, Priority::High);
    // urgent is the ceiling
    s.escalate_ticket(&t.id, "x", "s").unwrap(); // -> urgent
    let e3 = s.escalate_ticket(&t.id, "x", "s").unwrap(); // stays urgent
    assert_eq!(e3.priority, Priority::Urgent);
    assert_eq!(e3.escalation_level, 3);
}

#[test]
fn close_is_terminal() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "T", "b", "r", Priority::Normal, vec![], "a").unwrap();
    let closed = s.close_ticket(&t.id, "resolved by reset", "agent").unwrap();
    assert_eq!(closed.status, Status::Closed);
    assert!(closed.closed_at.is_some() && closed.resolved_at.is_some());
    // further transitions rejected
    assert!(s.set_status(&t.id, Status::Open, "a").is_err());
    assert!(s.close_ticket(&t.id, "again", "a").is_err());
    assert!(s.assign_ticket(&t.id, "x", "a").is_err());
}

#[test]
fn set_priority_recomputes_sla() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "T", "b", "r", Priority::Low, vec![], "a").unwrap();
    let low_due = t.resolution_due;
    let bumped = s.set_priority(&t.id, Priority::Urgent, "a").unwrap();
    assert!(bumped.resolution_due < low_due, "urgent should be sooner than low");
}

#[test]
fn sla_status_breach_detection() {
    let s = store();
    let q = first_queue(&s, "student-support");
    let t = s.create_ticket(&q, "T", "b", "r", Priority::Normal, vec![], "a").unwrap();
    // not breached at creation
    let st = s.sla_status(&t.id).unwrap();
    assert_eq!(st["response_breached"], false);
    assert_eq!(st["resolution_breached"], false);
    assert!(st["response_minutes_remaining"].as_i64().unwrap() > 0);
}

#[test]
fn sla_report_and_workload() {
    let s = store();
    let report = s.sla_report(None);
    assert!(report["total"].as_u64().unwrap() >= 4);
    assert!(report["open"].as_u64().unwrap() >= 1);
    let wl = s.workload();
    // seeded t1 assigned to support.advisor
    assert!(wl["open_by_assignee"].as_object().unwrap().contains_key("support.advisor"));
}

#[test]
fn tags_dedupe() {
    let s = store();
    let q = first_queue(&s, "privacy");
    let t = s.create_ticket(&q, "DSAR", "b", "subject", Priority::High, vec!["dsar".into()], "a").unwrap();
    s.add_tag(&t.id, "GDPR", "a").unwrap();
    let again = s.add_tag(&t.id, "gdpr", "a").unwrap();
    assert_eq!(again.tags.iter().filter(|x| *x == "gdpr").count(), 1);
}
