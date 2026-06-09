//! Validate mcp-server.toml parses, passes SDK validation, has the right tool
//! count, and gates the close + public-reply writes.

use adk_mcp_sdk::manifest::ServerManifest;
use std::path::Path;

fn manifest() -> ServerManifest {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("mcp-server.toml");
    ServerManifest::from_file(&path).expect("manifest should parse")
}

#[test]
fn manifest_parses_and_validates() {
    let m = manifest();
    assert!(m.validate().is_empty(), "validation errors: {:?}", m.validate());
    assert_eq!(m.server_id, "mcp_ticketing");
    assert_eq!(m.domain, "education");
    assert_eq!(m.tools.len(), 19, "expected 19 declared tools");
}

#[test]
fn gated_writes() {
    let m = manifest();
    for name in ["close_ticket", "send_public_reply"] {
        let t = m.tools.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("{name} present"));
        assert!(t.requires_approval, "{name} must require approval");
    }
}

#[test]
fn public_reply_is_external_write() {
    use adk_mcp_sdk::risk::RiskClass;
    let m = manifest();
    let t = m.tools.iter().find(|t| t.name == "send_public_reply").unwrap();
    assert_eq!(t.risk_class, RiskClass::ExternalWrite);
}

#[test]
fn analytics_reads_are_read_only() {
    use adk_mcp_sdk::risk::RiskClass;
    let m = manifest();
    for name in ["get_ticket", "list_tickets", "list_comments", "sla_status", "sla_report", "workload", "audit_log"] {
        let t = m.tools.iter().find(|t| t.name == name).unwrap();
        assert_eq!(t.risk_class, RiskClass::ReadOnly, "{name} should be read_only");
    }
}
