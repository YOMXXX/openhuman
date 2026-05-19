use super::{TaskProfile, TaskRiskLevel, ToolCapability, ToolPolicyAction, ToolPolicyDecision};
use crate::openhuman::tools::{PermissionLevel, Tool};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Builds deterministic per-session policy snapshots from the active agent,
/// channel, configured channel permissions, and available tool registry.
pub struct ToolPolicyEngine;

impl ToolPolicyEngine {
    /// Resolve the policy profile and per-tool decisions for one agent session.
    ///
    /// Empty `channel_permissions` preserves the legacy unrestricted tool
    /// surface. Once any channel policy exists, unknown channels fall back to
    /// read-only.
    pub fn build_session(
        agent_id: impl Into<String>,
        channel: impl Into<String>,
        entrypoint: impl Into<String>,
        channel_permissions: &HashMap<String, String>,
        tools: &[Box<dyn Tool>],
        visible_tool_names: &HashSet<String>,
    ) -> super::ToolPolicySession {
        let channel = channel.into();
        let allowed_permission = permission_for_channel(channel_permissions, &channel);
        let profile = TaskProfile {
            agent_id: agent_id.into(),
            channel,
            entrypoint: entrypoint.into(),
            risk_level: TaskRiskLevel::from_allowed_permission(allowed_permission),
            allowed_permission,
        };

        let mut allowed_tool_names = BTreeSet::new();
        let mut blocked_tool_names = BTreeSet::new();
        let mut capabilities = Vec::with_capacity(tools.len());
        let mut decisions = HashMap::with_capacity(tools.len());

        for tool in tools {
            let name = tool.name().to_string();
            let required_permission = tool.permission_level();
            let explicitly_hidden =
                !visible_tool_names.is_empty() && !visible_tool_names.contains(&name);
            let exceeds_permission = required_permission > allowed_permission;

            let action = if explicitly_hidden {
                ToolPolicyAction::HideFromPrompt
            } else if exceeds_permission {
                ToolPolicyAction::Deny
            } else {
                ToolPolicyAction::Allow
            };

            let capability = ToolCapability {
                name: name.clone(),
                required_permission,
            };

            match action {
                ToolPolicyAction::Allow => {
                    allowed_tool_names.insert(name.clone());
                }
                ToolPolicyAction::RequireApproval
                | ToolPolicyAction::Deny
                | ToolPolicyAction::HideFromPrompt => {
                    blocked_tool_names.insert(name.clone());
                }
            }

            decisions.insert(
                name.clone(),
                ToolPolicyDecision {
                    tool_name: name,
                    action,
                    required_permission: Some(required_permission),
                    allowed_permission,
                },
            );
            capabilities.push(capability);
        }

        super::ToolPolicySession {
            profile,
            capabilities,
            allowed_tool_names,
            blocked_tool_names,
            decisions,
        }
    }
}

fn permission_for_channel(
    channel_permissions: &HashMap<String, String>,
    channel: &str,
) -> PermissionLevel {
    if channel_permissions.is_empty() {
        return PermissionLevel::Dangerous;
    }

    channel_permissions
        .get(channel)
        .and_then(|value| parse_permission_level(value))
        .unwrap_or(PermissionLevel::ReadOnly)
}

fn parse_permission_level(value: &str) -> Option<PermissionLevel> {
    match value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_'], "")
        .as_str()
    {
        "none" => Some(PermissionLevel::None),
        "readonly" | "read" => Some(PermissionLevel::ReadOnly),
        "write" => Some(PermissionLevel::Write),
        "execute" | "exec" => Some(PermissionLevel::Execute),
        "dangerous" | "danger" => Some(PermissionLevel::Dangerous),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::tools::{PermissionLevel, Tool, ToolResult};
    use async_trait::async_trait;
    use std::collections::{HashMap, HashSet};

    struct PolicyTestTool {
        name: &'static str,
        permission: PermissionLevel,
    }

    #[async_trait]
    impl Tool for PolicyTestTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.name
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }

        async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::success("ok"))
        }

        fn permission_level(&self) -> PermissionLevel {
            self.permission
        }
    }

    fn tools() -> Vec<Box<dyn Tool>> {
        vec![
            Box::new(PolicyTestTool {
                name: "read_notes",
                permission: PermissionLevel::ReadOnly,
            }),
            Box::new(PolicyTestTool {
                name: "write_notes",
                permission: PermissionLevel::Write,
            }),
            Box::new(PolicyTestTool {
                name: "run_script",
                permission: PermissionLevel::Execute,
            }),
        ]
    }

    #[test]
    fn permission_from_channel_config_defaults_to_read_only() {
        let mut permissions = HashMap::new();
        permissions.insert("web".to_string(), "write".to_string());
        let session = ToolPolicyEngine::build_session(
            "orchestrator",
            "unknown-channel",
            "chat",
            &permissions,
            &tools(),
            &HashSet::new(),
        );

        assert_eq!(
            session.profile.allowed_permission,
            PermissionLevel::ReadOnly
        );
        assert!(session.is_allowed("read_notes"));
        assert!(!session.is_allowed("write_notes"));
    }

    #[test]
    fn empty_channel_config_preserves_legacy_full_tool_surface() {
        let session = ToolPolicyEngine::build_session(
            "orchestrator",
            "web",
            "chat",
            &HashMap::new(),
            &tools(),
            &HashSet::new(),
        );

        assert_eq!(
            session.profile.allowed_permission,
            PermissionLevel::Dangerous
        );
        assert!(session.is_allowed("read_notes"));
        assert!(session.is_allowed("write_notes"));
        assert!(session.is_allowed("run_script"));
        assert!(!session.has_restrictions());
    }

    #[test]
    fn filters_tools_above_channel_permission() {
        let mut permissions = HashMap::new();
        permissions.insert("web".to_string(), "write".to_string());

        let session = ToolPolicyEngine::build_session(
            "orchestrator",
            "web",
            "chat",
            &permissions,
            &tools(),
            &HashSet::new(),
        );

        assert!(session.is_allowed("read_notes"));
        assert!(session.is_allowed("write_notes"));
        assert!(!session.is_allowed("run_script"));
    }

    #[test]
    fn explicit_visible_names_still_narrow_policy_allowed_tools() {
        let mut permissions = HashMap::new();
        permissions.insert("cli".to_string(), "execute".to_string());
        let visible = HashSet::from(["run_script".to_string()]);

        let session = ToolPolicyEngine::build_session(
            "code_executor",
            "cli",
            "chat",
            &permissions,
            &tools(),
            &visible,
        );

        assert!(!session.is_allowed("read_notes"));
        assert!(!session.is_allowed("write_notes"));
        assert!(session.is_allowed("run_script"));
    }

    #[test]
    fn decision_denies_unknown_or_disallowed_tool() {
        let mut permissions = HashMap::new();
        permissions.insert("web".to_string(), "read_only".to_string());
        let session = ToolPolicyEngine::build_session(
            "orchestrator",
            "web",
            "chat",
            &permissions,
            &tools(),
            &HashSet::new(),
        );

        assert!(session.decision_for("write_notes").is_denied());
        assert!(session.decision_for("missing_tool").is_denied());
        assert!(!session.decision_for("read_notes").is_denied());
    }
}
