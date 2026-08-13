# Ticketing MCP Server

[![Crates.io](https://img.shields.io/crates/v/mcp-ticketing.svg)](https://crates.io/crates/mcp-ticketing)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![ADK-Rust Enterprise](https://img.shields.io/badge/ADK--Rust-Enterprise-purple.svg)](https://enterprise.adk-rust.com)
[![Registry Ready](https://img.shields.io/badge/ADK_Registry-Ready-green.svg)](https://www.zavora.ai)

A service-desk / case-ticketing platform for [ADK-Rust Enterprise](https://enterprise.adk-rust.com) support, legal, and operations agents. 19 MCP tools covering queues with **SLA policies**, tickets with priority and **auto-computed SLA due dates + breach detection**, intake & triage, assignment, a status workflow, **public/internal comments**, escalation, tags, and SLA/workload analytics — over an audit trail.

## A platform, not a point solution

This is modeled as a general ticketing backbone (à la Zendesk / Jira Service Management / ServiceNow), so support and case-management agents across domains are clients of one shared queue system:

| Agent | Domain | Uses |
|-------|--------|------|
| **Student Support Agent** | education | `create_ticket`, `assign_ticket`, `send_public_reply`, `sla_status` |
| **Matter Intake Agent** | legal | `create_ticket`, `add_tag`, `add_internal_note`, `list_tickets` |
| **Privacy Request Agent** | legal | `create_ticket` (DSAR), `escalate_ticket`, `sla_report`, `close_ticket` |

Different queues carry different **SLA policies** — e.g. fast student-support response targets, slower legal-intake review windows, and a ~30-day privacy/DSAR resolution clock.

## Architecture

<p align="center">
  <img src="https://raw.githubusercontent.com/zavora-ai/mcp-ticketing/main/docs/architecture.svg" alt="Ticketing MCP Architecture" width="780"/>
</p>

## Capabilities

- **Queues & SLA policy** — each queue defines per-priority response and resolution targets (hours).
- **Tickets** — intake with requester, priority, and tags; **SLA deadlines auto-computed** from the queue policy × priority. Status workflow: new → open → pending → resolved → closed (closed is terminal).
- **Triage & assignment** — assign to agents (auto-opens new tickets), set priority (recomputes SLA), tag, and escalate (raises priority a notch, bumps escalation level, re-baselines SLA).
- **Comments** — **internal notes** (staff-only) and **public replies** (visible to the requester; the first public reply sets the SLA first-response time).
- **SLA & analytics** — `sla_status` (per-ticket breach flags + minutes remaining), `sla_report` (open / response-breached / resolution-breached / at-risk-next-2h across a queue), and `workload` (open tickets per assignee).

## Governance posture

- **Two writes are gated** (`requires_approval`): `send_public_reply` (an external-facing message to the requester — classed `external_write`) and `close_ticket` (terminal disposition of a case). Internal notes, triage, and assignment are normal internal writes.
- **Workflow integrity** — a closed ticket can't be reassigned, re-transitioned, or re-closed; everything is on the audit trail (`audit_log`).
- **Reads are `read_only`**. Sample data is fictitious.

## Tools (19)

### Queues (3)
`create_queue` · `get_queue` · `list_queues`

### Tickets & Workflow (9)
`create_ticket` · `get_ticket` · `list_tickets` · `assign_ticket` · `set_priority` · `set_status` · `close_ticket` (gated) · `add_tag` · `escalate_ticket`

### Comments (3)
`add_internal_note` · `send_public_reply` (gated, external) · `list_comments`

### SLA & Analytics (4)
`sla_status` · `sla_report` · `workload` · `audit_log`

## Example

```jsonc
// Intake + triage
{"name": "create_ticket", "arguments": {"queue_id": "QUE-1000", "subject": "Locked out of portal",
  "requester": "stu-2001", "priority": "high", "tags": ["access"]}}
{"name": "assign_ticket", "arguments": {"ticket_id": "TKT-1008", "assignee": "advisor.kim"}}

// Gated public reply, then SLA check
{"name": "send_public_reply", "arguments": {"ticket_id": "TKT-1008", "body": "Reset link sent."}}
{"name": "sla_status", "arguments": {"ticket_id": "TKT-1008"}}

// Privacy: escalate a DSAR, then close (gated)
{"name": "escalate_ticket", "arguments": {"ticket_id": "TKT-1011", "reason": "regulatory clock"}}
{"name": "close_ticket", "arguments": {"ticket_id": "TKT-1008", "resolution": "password reset confirmed"}}
```

## Install & run

```bash
cargo install mcp-ticketing
mcp-ticketing            # serves MCP over stdio
```

Or build from source:

```bash
git clone https://github.com/zavora-ai/mcp-ticketing
cd mcp-ticketing && cargo build --release
./target/release/mcp-ticketing
```

## Registry manifest

```toml
server_id = "mcp_ticketing"
display_name = "Ticketing / Service Desk"
version = "1.0.0"
domain = "education"
risk_level = "medium"
writes_allowed = "gated"
```

The full [`mcp-server.toml`](mcp-server.toml) declares all 19 tools with risk classes and approval gates for registry onboarding.

## License

Apache-2.0

## rmcp and MCP compatibility

This server is built with [`rmcp` 3.1.2](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.1.2) and requires Rust 1.94.1 or newer. The rmcp 3 rollout retains legacy MCP initialization compatibility and targets MCP protocol revisions `2025-11-25` and `2026-07-28`.

## MCP 2026-07-28 rollout (P4 workflow/business)

This server uses `rmcp` 3.1.2 and `adk-mcp-sdk` 0.2 with a minimum supported
Rust version of **1.94.1**. It accepts stateless MCP 2026 requests with
per-request protocol, client identity, and capability metadata while retaining
the legacy MCP 2025-11-25 initialize flow for ordinary tools.

- **Tasks:** None; this server's operations are short-lived and execute directly.
- **MRTR approvals:** `close_ticket`, `send_public_reply`
- **Discovery and routing:** rmcp serves on-demand discovery and validates the
  per-request protocol envelope; HTTP deployments can route with `Mcp-Method`
  and `Mcp-Name`. The packaged binary currently uses stdio.
- **Caching:** `tools/list` returns a public `ttlMs` of 60,000 for MCP 2026;
  rmcp omits the cache fields for legacy clients.
- **Deprecated extensions:** this server does not add new Roots, Sampling, or
  dynamic client-registration dependencies.

Protected tools require `MCP_REQUEST_STATE_KEY` with at least 32 high-entropy
bytes. All replicas must share that key so sealed approval state can resume on
another instance. Approval state is bound to the client identity, tool, and
arguments and expires after two minutes. Missing identity, invalid state,
rejection, or legacy protocol use fails closed. Task records are process-local
for the current stdio runtime; use a durable task store before deploying the
server behind scale-to-zero HTTP infrastructure.
