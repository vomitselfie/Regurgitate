use serde_json::{Value, json};

use crate::application::{ComponentReadiness, HealthReport, OverallHealth};

use super::{
    PluginSnapshot,
    setup::{IntegrationReadiness, IntegrationState, SetupNotice, SetupOutcome, SetupTarget},
};

pub(super) fn status_bar(report: &HealthReport) -> Value {
    let text = match (report.status, report.history.event_count) {
        (OverallHealth::Ready, Some(events)) => format!("Praxis {events}"),
        (OverallHealth::Ready, None) => "Praxis ready".to_owned(),
        (OverallHealth::NotConfigured, _) => "Praxis not configured".to_owned(),
        (OverallHealth::Degraded, _) => "Praxis degraded".to_owned(),
    };
    json!({
        "text": text,
        "tone": overall_tone(report.status),
        "tooltip": "Encrypted procedural-memory health; no prompts or tool output are exposed.",
        "icon": "brain-circuit"
    })
}

pub(super) fn settings_page(snapshot: &PluginSnapshot, last_setup: Option<SetupNotice>) -> Value {
    let report = &snapshot.health;
    let any_connected = [snapshot.integrations.codex, snapshot.integrations.claude]
        .into_iter()
        .any(integration_ready);
    let (title, detail, tone) = match report.status {
        OverallHealth::Ready => (
            "Praxis is ready",
            "Encrypted procedural observations are available to bounded agent recall.",
            "success",
        ),
        OverallHealth::NotConfigured if any_connected => (
            "Praxis is connected",
            "Use your agent normally; the first supported event will initialize encrypted history.",
            "info",
        ),
        OverallHealth::NotConfigured => (
            "Praxis is not configured yet",
            "Choose an agent below. Praxis will install its recording hook and recall skill.",
            "neutral",
        ),
        OverallHealth::Degraded => (
            "Praxis needs attention",
            "The operating system credential store or encrypted history database is unavailable.",
            "warn",
        ),
    };

    let mut blocks = vec![
        json!({"kind": "heading", "text": "Private procedural memory"}),
        json!({"kind": "callout", "title": title, "detail": detail, "tone": tone}),
    ];
    if let Some(notice) = last_setup {
        blocks.push(setup_notice(notice));
    }
    blocks.extend([
        json!({
            "kind": "section",
            "title": "Connect an agent",
            "children": [
                {
                    "kind": "row",
                    "label": "Codex",
                    "value": integration_label(snapshot.integrations.codex),
                    "value_tone": integration_tone(snapshot.integrations.codex)
                },
                {
                    "kind": "row",
                    "label": "Claude Code",
                    "value": integration_label(snapshot.integrations.claude),
                    "value_tone": integration_tone(snapshot.integrations.claude)
                }
            ]
        }),
        json!({
            "kind": "columns",
            "children": [
                {
                    "kind": "action",
                    "label": "Set up Codex",
                    "method": "praxis.setup.codex",
                    "icon": "plug",
                    "variant": "primary"
                },
                {
                    "kind": "action",
                    "label": "Set up Claude Code",
                    "method": "praxis.setup.claude",
                    "icon": "plug"
                }
            ]
        }),
        json!({
            "kind": "note",
            "text": "Setup changes only the selected agent's user hook and Praxis skill. Existing settings and personal hooks are preserved. Restart that agent after setup.",
            "tone": "info"
        }),
        json!({
            "kind": "section",
            "title": "Local state",
            "children": [
                {
                    "kind": "row",
                    "label": "Overall",
                    "value": overall_label(report.status),
                    "value_tone": overall_tone(report.status)
                },
                {
                    "kind": "row",
                    "label": "Key store",
                    "value": component_label(report.key_store),
                    "value_tone": component_tone(report.key_store)
                },
                {
                    "kind": "row",
                    "label": "Encrypted history",
                    "value": history_label(report),
                    "value_tone": component_tone(report.history.status)
                }
            ]
        }),
        json!({
            "kind": "note",
            "text": "Recording stays provider-native. Praxis exposes only controlled aggregate health to AoE and never sends prompts, responses, commands, paths, or tool output.",
            "tone": "info"
        }),
        json!({
            "kind": "action",
            "label": "Refresh",
            "method": "praxis.refresh",
            "icon": "refresh-cw"
        }),
    ]);

    json!({
        "title": "Praxis",
        "icon": "brain-circuit",
        "blocks": blocks
    })
}

fn setup_notice(notice: SetupNotice) -> Value {
    let agent = match notice.target {
        SetupTarget::Codex => "Codex",
        SetupTarget::Claude => "Claude Code",
    };
    let (title, detail, tone) = match notice.outcome {
        SetupOutcome::Installed => (
            format!("{agent} setup complete"),
            "The recording hook and recall skill were installed. Restart the agent to load them.",
            "success",
        ),
        SetupOutcome::AlreadyCurrent => (
            format!("{agent} is already connected"),
            "No files needed to change.",
            "success",
        ),
        SetupOutcome::Failed => (
            format!("{agent} setup needs attention"),
            "Praxis preserved the existing configuration. Review that agent's user settings or use the standalone installer for details.",
            "warn",
        ),
    };
    json!({"kind": "callout", "title": title, "detail": detail, "tone": tone})
}

fn integration_ready(state: IntegrationState) -> bool {
    state.hook == IntegrationReadiness::Ready && state.skill == IntegrationReadiness::Ready
}

fn integration_label(state: IntegrationState) -> &'static str {
    if integration_ready(state) {
        "connected"
    } else if matches!(
        (state.hook, state.skill),
        (IntegrationReadiness::NeedsAttention, _) | (_, IntegrationReadiness::NeedsAttention)
    ) {
        "needs attention"
    } else {
        "not connected"
    }
}

fn integration_tone(state: IntegrationState) -> &'static str {
    match integration_label(state) {
        "connected" => "success",
        "needs attention" => "warn",
        _ => "neutral",
    }
}

fn overall_label(readiness: OverallHealth) -> &'static str {
    match readiness {
        OverallHealth::Ready => "ready",
        OverallHealth::NotConfigured => "not configured",
        OverallHealth::Degraded => "degraded",
    }
}

fn overall_tone(readiness: OverallHealth) -> &'static str {
    match readiness {
        OverallHealth::Ready => "success",
        OverallHealth::NotConfigured => "neutral",
        OverallHealth::Degraded => "warn",
    }
}

fn component_label(readiness: ComponentReadiness) -> &'static str {
    match readiness {
        ComponentReadiness::Ready => "ready",
        ComponentReadiness::NotConfigured => "not configured",
        ComponentReadiness::Unavailable => "unavailable",
    }
}

fn component_tone(readiness: ComponentReadiness) -> &'static str {
    match readiness {
        ComponentReadiness::Ready => "success",
        ComponentReadiness::NotConfigured => "neutral",
        ComponentReadiness::Unavailable => "warn",
    }
}

fn history_label(report: &HealthReport) -> String {
    match (report.history.status, report.history.event_count) {
        (ComponentReadiness::Ready, Some(events)) => format!("{events} events"),
        (readiness, _) => component_label(readiness).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aoe_plugin::setup::IntegrationOverview;
    use crate::application::HistoryHealth;

    fn report(status: OverallHealth, component: ComponentReadiness) -> HealthReport {
        HealthReport {
            status,
            key_store: component,
            history: HistoryHealth {
                status: component,
                event_count: (component == ComponentReadiness::Ready).then_some(493),
            },
            hooks: vec![],
        }
    }

    fn snapshot(status: OverallHealth, component: ComponentReadiness) -> PluginSnapshot {
        PluginSnapshot {
            health: report(status, component),
            integrations: IntegrationOverview {
                codex: IntegrationState {
                    hook: IntegrationReadiness::NotConfigured,
                    skill: IntegrationReadiness::NotConfigured,
                },
                claude: IntegrationState {
                    hook: IntegrationReadiness::NotConfigured,
                    skill: IntegrationReadiness::NotConfigured,
                },
            },
        }
    }

    #[test]
    fn ready_view_contains_only_controlled_aggregate_health() {
        let snapshot = snapshot(OverallHealth::Ready, ComponentReadiness::Ready);
        assert_eq!(status_bar(&snapshot.health)["text"], json!("Praxis 493"));
        assert_eq!(status_bar(&snapshot.health)["tone"], json!("success"));

        let page = settings_page(&snapshot, None);
        assert_eq!(page["title"], json!("Praxis"));
        assert_eq!(page["blocks"][1]["title"], json!("Praxis is ready"));
        assert_eq!(
            page["blocks"][5]["children"][2]["value"],
            json!("493 events")
        );
        let encoded = serde_json::to_string(&page).unwrap();
        for forbidden in ["/home/", "prompt text", "tool output value"] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn unavailable_components_render_as_controlled_degraded_state() {
        let snapshot = snapshot(OverallHealth::Degraded, ComponentReadiness::Unavailable);
        assert_eq!(
            status_bar(&snapshot.health)["text"],
            json!("Praxis degraded")
        );
        assert_eq!(status_bar(&snapshot.health)["tone"], json!("warn"));
        assert_eq!(
            settings_page(&snapshot, None)["blocks"][5]["children"][1]["value"],
            json!("unavailable")
        );
    }

    #[test]
    fn setup_actions_and_bounded_result_are_visible_without_paths() {
        let snapshot = snapshot(
            OverallHealth::NotConfigured,
            ComponentReadiness::NotConfigured,
        );
        let page = settings_page(
            &snapshot,
            Some(SetupNotice {
                target: SetupTarget::Codex,
                outcome: SetupOutcome::Installed,
            }),
        );
        let encoded = serde_json::to_string(&page).unwrap();
        assert!(encoded.contains("praxis.setup.codex"));
        assert!(encoded.contains("praxis.setup.claude"));
        assert!(encoded.contains("Codex setup complete"));
        assert!(!encoded.contains("/home/"));
    }
}
