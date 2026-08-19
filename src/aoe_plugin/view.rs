use serde_json::{Value, json};

use crate::application::{ComponentReadiness, HealthReport, OverallHealth};

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

pub(super) fn settings_page(report: &HealthReport) -> Value {
    let (title, detail, tone) = match report.status {
        OverallHealth::Ready => (
            "Praxis is ready",
            "Encrypted procedural observations are available to bounded agent recall.",
            "success",
        ),
        OverallHealth::NotConfigured => (
            "Praxis is not configured yet",
            "The first verified hook event or learned strategy will initialize encrypted history.",
            "neutral",
        ),
        OverallHealth::Degraded => (
            "Praxis needs attention",
            "Linux Secret Service or the encrypted history database is unavailable.",
            "warn",
        ),
    };

    json!({
        "title": "Praxis",
        "icon": "brain-circuit",
        "blocks": [
            {"kind": "heading", "text": "Private procedural memory"},
            {"kind": "callout", "title": title, "detail": detail, "tone": tone},
            {
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
            },
            {
                "kind": "note",
                "text": "Recording remains provider-native. This plugin exposes controlled aggregate health and never sends prompts, responses, commands, paths, or tool output to AoE.",
                "tone": "info"
            },
            {
                "kind": "action",
                "label": "Refresh",
                "method": "praxis.refresh",
                "icon": "refresh-cw"
            }
        ]
    })
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

    #[test]
    fn ready_view_contains_only_controlled_aggregate_health() {
        let report = report(OverallHealth::Ready, ComponentReadiness::Ready);
        assert_eq!(status_bar(&report)["text"], json!("Praxis 493"));
        assert_eq!(status_bar(&report)["tone"], json!("success"));

        let page = settings_page(&report);
        assert_eq!(page["title"], json!("Praxis"));
        assert_eq!(page["blocks"][1]["title"], json!("Praxis is ready"));
        assert_eq!(
            page["blocks"][2]["children"][2]["value"],
            json!("493 events")
        );
        let encoded = serde_json::to_string(&page).unwrap();
        for forbidden in ["/home/", "prompt text", "tool output value"] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn unavailable_components_render_as_controlled_degraded_state() {
        let report = report(OverallHealth::Degraded, ComponentReadiness::Unavailable);
        assert_eq!(status_bar(&report)["text"], json!("Praxis degraded"));
        assert_eq!(status_bar(&report)["tone"], json!("warn"));
        assert_eq!(
            settings_page(&report)["blocks"][2]["children"][1]["value"],
            json!("unavailable")
        );
    }
}
