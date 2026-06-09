//! Ticketing MCP Server library surface.
//!
//! A service-desk / case-ticketing platform: queues with SLA policies, tickets
//! with priority + computed SLA due dates and breach detection, intake & triage,
//! assignment, status workflow, public/internal comments, escalation, tags, and
//! SLA/workload analytics — over an audit trail.

pub mod server;
pub mod store;
pub mod types;
