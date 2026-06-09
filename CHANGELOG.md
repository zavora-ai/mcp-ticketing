# Changelog

## [1.0.0] - 2026-06-10

Initial release — a broad service-desk / ticketing platform with SLA tracking.

### Added
- **Queues & SLA policy** — per-priority response/resolution targets
  (`create_queue`, `get_queue`, `list_queues`)
- **Tickets & workflow** — intake with auto-computed SLA deadlines, status workflow (closed terminal), assignment, priority (recomputes SLA), tags, and escalation (raises priority + re-baselines SLA)
  (`create_ticket`, `get_ticket`, `list_tickets`, `assign_ticket`, `set_priority`, `set_status`, `close_ticket`, `add_tag`, `escalate_ticket`)
- **Comments** — internal staff notes and public replies (first public reply sets SLA first-response)
  (`add_internal_note`, `send_public_reply`, `list_comments`)
- **SLA & analytics** — per-ticket breach status, queue SLA report (breaches + at-risk), and workload by assignee
  (`sla_status`, `sla_report`, `workload`, `audit_log`)
- 19 tools total; `close_ticket` and `send_public_reply` (external-facing reply) are approval-gated; full audit trail.
- 15 tests (11 integration + 4 manifest); verified end-to-end over MCP stdio.
