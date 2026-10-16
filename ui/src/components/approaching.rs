//! Approaching (spec 45): a strip that says when trouble elsewhere will reach what you are looking
//! at (a project, your week, an objective), and a list of the chains behind it. The rules are in
//! `minimap_core::approach`; this file asks for the report and says it in words.
//!
//! The strip is hidden when nothing is hitting or threatening, so there is no "all clear" to read.
//! Things that are only worth watching are in the list, behind a button, and never in the strip.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use minimap_types::{
    ApproachGroup, ApproachReport, ApproachScope, Approaching, Headline, NodeSummary, Reason,
    Severity,
};

use crate::{
    api,
    components::{
        form::BUTTON,
        objective_colour::{use_objective_colours, ObjectiveChips},
        page::{Icon, Tone},
        what_if_button::DEFAULT_SLIP_DAYS,
    },
    state::{DataVersion, Scenario, Selection},
};

// ------------------------------------------------------------------ pure text

/// "1 day", "4 days".
fn days(n: u32) -> String {
    if n == 1 {
        "1 day".to_owned()
    } else {
        format!("{n} days")
    }
}

/// "today", "in 1 day", "in 4 days": when the first of them arrives.
pub fn when_text(first_in_days: u32) -> String {
    match first_in_days {
        0 => "today".to_owned(),
        n => format!("in {}", days(n)),
    }
}

/// The strip's words: "2 approaching from EU Region · first in 4 days" or "3 approaching from 2
/// projects · first today". Only what is hitting or threatening counts, like the headline.
pub fn headline_text(headline: Headline, groups: &[ApproachGroup]) -> String {
    let sources: Vec<&str> = groups
        .iter()
        .filter(|g| g.items.iter().any(|i| i.severity != Severity::Watch))
        .map(|g| {
            g.source_project
                .as_ref()
                .map_or("the inbox", |p| p.label.as_str())
        })
        .collect();
    let from = match sources.as_slice() {
        [one] => (*one).to_owned(),
        many => format!("{} projects", many.len()),
    };
    format!(
        "{} approaching from {from} · first {}",
        headline.count,
        when_text(headline.first_in_days)
    )
}

/// What a reason says: "Overdue 3 days", "Blocked", "No slack".
pub fn reason_text(reason: Reason, overdue_days: Option<u32>) -> String {
    match (reason, overdue_days) {
        (Reason::Overdue, Some(n)) => format!("Overdue {}", days(n)),
        (Reason::Overdue, None) => "Overdue".to_owned(),
        (Reason::Blocked, _) => "Blocked".to_owned(),
        (Reason::Late, _) => "Late".to_owned(),
        (Reason::NoSlack, _) => "No slack".to_owned(),
        (Reason::ProjectLate, _) => "Project late".to_owned(),
        (Reason::ProjectRed, _) => "Health red".to_owned(),
    }
}

/// Overdue is red; the rest need attention.
pub fn reason_tone(reason: Reason) -> Tone {
    match reason {
        Reason::Overdue => Tone::Danger,
        _ => Tone::Warning,
    }
}

/// The chain in words: "Gateway auth → API migration → Pen test".
pub fn path_text(path: &[NodeSummary]) -> String {
    path.iter()
        .map(|n| n.label.as_str())
        .collect::<Vec<_>>()
        .join(" → ")
}

/// "reaches Pen test now" / "reaches Pen test in 3 working days".
pub fn arrives_text(target: &str, arrives_in: u32) -> String {
    match arrives_in {
        0 => format!("reaches {target} now"),
        1 => format!("reaches {target} in 1 working day"),
        n => format!("reaches {target} in {n} working days"),
    }
}

/// What the target has to give: "Pen test has 1 day of slack".
pub fn slack_text(target: &str, slack: Option<i32>) -> Option<String> {
    slack.map(|s| match s {
        s if s < 0 => format!("{target} is already {} late", days(s.unsigned_abs())),
        0 => format!("{target} has no slack"),
        s => format!("{target} has {} of slack", days(s as u32)),
    })
}

/// The severity in words, so the colour is never the only signal.
pub fn severity_text(severity: Severity) -> &'static str {
    match severity {
        Severity::Hits => "Holding it up",
        Severity::Threatens => "Threatens it",
        Severity::Watch => "Worth watching",
    }
}

pub fn severity_tone(severity: Severity) -> Tone {
    match severity {
        Severity::Hits => Tone::Danger,
        Severity::Threatens => Tone::Warning,
        Severity::Watch => Tone::Neutral,
    }
}

/// The days to try in "What if it slips?": how overdue it is, or the usual five.
pub fn slip_days(item: &Approaching) -> u32 {
    item.overdue_days.unwrap_or(DEFAULT_SLIP_DAYS).clamp(1, 30)
}

// ------------------------------------------------------------------ the strip

/// The strip for `scope`, with its list of chains one click away. Nothing at all when nothing is
/// hitting or threatening. `class` is put on a wrapper that is only there while the strip is.
#[component]
pub fn ApproachStrip(scope: ApproachScope, #[prop(optional)] class: &'static str) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let report = RwSignal::new(None::<ApproachReport>);
    let loaded = LocalResource::new(move || {
        version.track();
        api::get_approaching(scope)
    });
    // Keep the last answer while reloading, so the strip never blinks after an edit.
    Effect::new(move |_| {
        if let Some(Ok(r)) = loaded.get() {
            report.set(Some(r));
        }
    });
    let open = RwSignal::new(false);
    move || {
        let r = report.get()?;
        let headline = r.headline?;
        let text = headline_text(headline, &r.groups);
        let tone = severity_tone(headline.worst);
        Some(view! {
            <div class=class>
                <div class="overflow-hidden rounded-sm border border-line bg-panel">
                    <button type="button"
                            class="flex h-7 w-full items-center gap-2 px-3 text-left text-[12px] hover:bg-hover"
                            aria-expanded=move || open.get().to_string()
                            title="Show the chains that lead here"
                            on:click=move |_| open.update(|o| *o = !*o)>
                        <span class=tone.text()><Icon name="approach" size="h-3.5 w-3.5" /></span>
                        <span class=format!("min-w-0 truncate font-medium {}", tone.text())>{text}</span>
                        <span class="ml-auto shrink-0 text-[11px] text-muted">
                            {move || if open.get() { "Hide" } else { "Show the chains" }}
                        </span>
                    </button>
                    {move || open.get().then(|| view! { <Chains groups=r.groups.clone() /> })}
                </div>
            </div>
        })
    }
}

/// The strip for the project a screen is filtered to (the Tasks board and list), if it is
/// filtered to one: under the filter bar, nothing otherwise.
#[component]
pub fn FilteredApproach(project: RwSignal<String>) -> impl IntoView {
    move || {
        minimap_types::Uuid::parse_str(&project.get()).ok().map(|id| {
            view! { <ApproachStrip scope=ApproachScope::Project(id) class="border-b border-line px-3 py-2" /> }
        })
    }
}

/// The chains behind the strip: what is hitting or threatening, group by group, and under a
/// button what is only worth watching.
#[component]
fn Chains(groups: Vec<ApproachGroup>) -> impl IntoView {
    let colours = use_objective_colours();
    let watching = RwSignal::new(false);
    let watch: Vec<Approaching> = groups
        .iter()
        .flat_map(|g| g.items.iter())
        .filter(|i| i.severity == Severity::Watch)
        .cloned()
        .collect();
    let watch_count = watch.len();
    let blocks = groups
        .into_iter()
        .filter_map(|g| {
            let loud: Vec<Approaching> = g
                .items
                .into_iter()
                .filter(|i| i.severity != Severity::Watch)
                .collect();
            (!loud.is_empty()).then(|| {
                let (name, objectives) = match &g.source_project {
                    Some(p) => (p.label.clone(), colours.of_project(Some(p.node.id))),
                    None => ("No project".to_owned(), Vec::new()),
                };
                view! {
                    <div class="flex items-center gap-2 border-t border-line px-3 py-1.5">
                        <span class="text-[10px] font-semibold uppercase tracking-wider text-muted">"From"</span>
                        <span class="text-[12px] font-medium">{name}</span>
                        <ObjectiveChips objectives=objectives max=1 />
                    </div>
                    {loud.into_iter().map(|i| view! { <ChainItem item=i /> }).collect_view()}
                }
            })
        })
        .collect_view();
    view! {
        {blocks}
        {(watch_count > 0).then(|| view! {
            <div class="border-t border-line px-3 py-1.5">
                <button type="button" class="text-[12px] text-muted hover:text-fg"
                        aria-expanded=move || watching.get().to_string()
                        on:click=move |_| watching.update(|w| *w = !*w)>
                    {move || format!("{} Also watching ({watch_count})", if watching.get() { "▾" } else { "▸" })}
                </button>
            </div>
            {
                let watch = watch.clone();
                move || watching.get().then(|| watch.clone().into_iter()
                    .map(|i| view! { <ChainItem item=i /> }).collect_view())
            }
        })}
    }
}

/// One chain: the path, why the source is in trouble, when it arrives and what to do.
#[component]
fn ChainItem(item: Approaching) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let scenario = expect_context::<Scenario>();
    let navigate = use_navigate();
    let source = item.source.node;
    let days_to_try = slip_days(&item);
    let steps = item
        .path
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let last = i + 1 == item.path.len();
            view! {
                {(i > 0).then(|| view! { <span class="text-muted">"→"</span> })}
                <span class=if last { "font-semibold" } else { "" }>{n.label.clone()}</span>
            }
        })
        .collect_view();
    let pills = item
        .reasons
        .iter()
        .map(|r| {
            let text = reason_text(*r, item.overdue_days);
            view! { <span class=reason_tone(*r).chip()>{text}</span> }
        })
        .collect_view();
    let arrives = arrives_text(&item.target.label, item.arrives_in);
    let slack = slack_text(&item.target.label, item.target_slack);
    let severity = item.severity;
    view! {
        <div class="border-t border-line px-3 py-2 text-[12px]">
            <div class="flex flex-wrap items-center gap-x-1.5 gap-y-0.5" title=path_text(&item.path)>{steps}</div>
            <div class="mt-1 flex flex-wrap items-center gap-1.5">
                {pills}
                <span class=format!("text-[11px] font-medium {}", severity_tone(severity).text())>
                    {severity_text(severity)}
                </span>
            </div>
            <p class="mt-1 text-muted">
                {arrives}
                {slack.map(|s| format!(". {s}"))}
            </p>
            <div class="mt-1.5 flex flex-wrap items-center gap-2">
                <button class=BUTTON on:click=move |_| selection.open(source)>"Open"</button>
                <button class=BUTTON title="See what it would push back if it slipped"
                        on:click=move |_| {
                            scenario.start(source, days_to_try);
                            navigate("/what-if", Default::default());
                        }>"What if it slips?"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeType, Uuid};

    fn node(label: &str) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Task, Uuid::nil()),
            label: label.into(),
            archived: false,
        }
    }

    fn item(severity: Severity) -> Approaching {
        Approaching {
            source: node("Gateway auth"),
            target: node("Pen test"),
            path: vec![node("Gateway auth"), node("Pen test")],
            reasons: vec![Reason::Overdue],
            overdue_days: Some(3),
            arrives_in: 4,
            severity,
            target_slack: Some(1),
        }
    }

    fn group(project: Option<&str>, severities: &[Severity]) -> ApproachGroup {
        ApproachGroup {
            source_project: project.map(node),
            items: severities.iter().map(|s| item(*s)).collect(),
        }
    }

    #[test]
    fn the_strip_names_the_project_or_counts_them_and_says_when() {
        let one = [group(
            Some("EU Region"),
            &[Severity::Hits, Severity::Threatens],
        )];
        let h = Headline {
            count: 2,
            first_in_days: 4,
            worst: Severity::Hits,
        };
        assert_eq!(
            headline_text(h, &one),
            "2 approaching from EU Region · first in 4 days"
        );
        let two = [
            group(Some("EU Region"), &[Severity::Threatens]),
            group(
                Some("Platform"),
                &[Severity::Threatens, Severity::Threatens],
            ),
        ];
        let h = Headline {
            count: 3,
            first_in_days: 0,
            worst: Severity::Threatens,
        };
        assert_eq!(
            headline_text(h, &two),
            "3 approaching from 2 projects · first today"
        );
        // A project that is only worth watching is not counted, and tasks in no project are the inbox.
        let mixed = [
            group(None, &[Severity::Hits]),
            group(Some("Quiet"), &[Severity::Watch]),
        ];
        let h = Headline {
            count: 1,
            first_in_days: 1,
            worst: Severity::Hits,
        };
        assert_eq!(
            headline_text(h, &mixed),
            "1 approaching from the inbox · first in 1 day"
        );
    }

    #[test]
    fn each_reason_says_what_is_wrong_and_overdue_is_the_red_one() {
        assert_eq!(reason_text(Reason::Overdue, Some(3)), "Overdue 3 days");
        assert_eq!(reason_text(Reason::Overdue, Some(1)), "Overdue 1 day");
        assert_eq!(reason_text(Reason::Overdue, None), "Overdue");
        assert_eq!(reason_text(Reason::Blocked, None), "Blocked");
        assert_eq!(reason_text(Reason::NoSlack, None), "No slack");
        assert_eq!(reason_text(Reason::ProjectLate, None), "Project late");
        assert_eq!(reason_text(Reason::ProjectRed, None), "Health red");
        assert_eq!(reason_tone(Reason::Overdue), Tone::Danger);
        for r in [
            Reason::Blocked,
            Reason::Late,
            Reason::NoSlack,
            Reason::ProjectLate,
        ] {
            assert_eq!(reason_tone(r), Tone::Warning);
        }
    }

    #[test]
    fn the_chain_and_its_timing_read_in_words() {
        let path = [
            node("Gateway auth"),
            node("API migration"),
            node("Pen test"),
        ];
        assert_eq!(path_text(&path), "Gateway auth → API migration → Pen test");
        assert_eq!(arrives_text("Pen test", 0), "reaches Pen test now");
        assert_eq!(
            arrives_text("Pen test", 1),
            "reaches Pen test in 1 working day"
        );
        assert_eq!(
            arrives_text("Pen test", 3),
            "reaches Pen test in 3 working days"
        );
        assert_eq!(slack_text("Pen test", None), None);
        assert_eq!(
            slack_text("Pen test", Some(1)).as_deref(),
            Some("Pen test has 1 day of slack")
        );
        assert_eq!(
            slack_text("Pen test", Some(0)).as_deref(),
            Some("Pen test has no slack")
        );
        assert_eq!(
            slack_text("Pen test", Some(-2)).as_deref(),
            Some("Pen test is already 2 days late")
        );
    }

    #[test]
    fn severity_is_said_in_words_and_what_if_starts_from_how_late_it_is() {
        assert_eq!(severity_text(Severity::Hits), "Holding it up");
        assert_eq!(severity_tone(Severity::Hits), Tone::Danger);
        assert_eq!(severity_tone(Severity::Threatens), Tone::Warning);
        assert_eq!(severity_tone(Severity::Watch), Tone::Neutral);
        assert_eq!(slip_days(&item(Severity::Hits)), 3);
        let mut blocked = item(Severity::Threatens);
        blocked.overdue_days = None;
        assert_eq!(slip_days(&blocked), DEFAULT_SLIP_DAYS);
        blocked.overdue_days = Some(400);
        assert_eq!(slip_days(&blocked), 30);
    }
}
