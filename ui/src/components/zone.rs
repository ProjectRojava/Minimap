//! The safe zone (spec 44): the room a project has between when it will really finish and when it
//! has to, in working days, as a thin bar and words. The numbers come from the backend
//! (`ProjectRow.zone`, `ProjectDetail.zone`, `ProjectHealthRow.zone`); this file only draws them.
//! The words carry the meaning and the colour is second, so the bar is never colour alone.

use leptos::prelude::*;
use minimap_types::{Zone, ZoneLevel};

use crate::components::page::Tone;

/// "1 day", "6 days".
fn days(n: u32) -> String {
    if n == 1 {
        "1 day".to_owned()
    } else {
        format!("{n} days")
    }
}

/// What the zone says, or nothing when there is nothing to show: "6 days of room", "No room
/// left", "3 days past target", "No target", "All tasks done".
pub fn zone_text(zone: Zone) -> Option<String> {
    match zone {
        Zone::None => None,
        Zone::AllDone { .. } => Some("All tasks done".to_owned()),
        Zone::NoTarget { .. } => Some("No target".to_owned()),
        Zone::Inside { room: 0, .. } => Some("No room left".to_owned()),
        Zone::Inside { room, .. } => Some(format!("{} of room", days(room))),
        Zone::Outside { over, .. } => Some(format!("{} past target", days(over))),
    }
}

/// How worried the zone looks: plenty of room is good, little is a warning, past the target is bad.
pub fn zone_tone(zone: Zone) -> Tone {
    match zone {
        Zone::None | Zone::NoTarget { .. } => Tone::Neutral,
        Zone::AllDone { .. } => Tone::Success,
        Zone::Inside {
            level: ZoneLevel::Wide,
            ..
        } => Tone::Success,
        Zone::Inside { .. } => Tone::Warning,
        Zone::Outside { .. } => Tone::Danger,
    }
}

/// The two parts of the track as fractions of its width: the work ahead, then the room (inside) or
/// the overflow (outside, where the track is the whole forecast and the target is where they meet).
/// Both are 0 when there is nothing to draw.
pub fn fill_fractions(zone: Zone) -> (f64, f64) {
    match zone {
        Zone::None | Zone::NoTarget { .. } => (0.0, 0.0),
        // Nothing left to do: the whole track.
        Zone::AllDone { .. } => (1.0, 0.0),
        Zone::Inside {
            remaining, room, ..
        } => {
            let total = f64::from(remaining) + f64::from(room);
            if total == 0.0 {
                (0.0, 0.0)
            } else {
                (f64::from(remaining) / total, f64::from(room) / total)
            }
        }
        Zone::Outside { remaining, over } => {
            let total = f64::from(remaining).max(f64::from(over)).max(1.0);
            let within = (total - f64::from(over)).max(0.0);
            (within / total, f64::from(over) / total)
        }
    }
}

/// The tooltip of the bar: the words and what they are measured in.
pub fn zone_hint(zone: Zone) -> Option<String> {
    let text = zone_text(zone)?;
    Some(match zone {
        Zone::AllDone { finished } => format!(
            "{text}: the last one was finished on {finished}. Mark the project done when it is finished"
        ),
        Zone::NoTarget { remaining } => {
            format!("{text}: {} of work ahead, and no target date to measure it against", days(remaining))
        }
        Zone::Inside { remaining, .. } => format!(
            "{text}: working days between when it should finish ({} of work ahead) and its target date",
            days(remaining)
        ),
        Zone::Outside { .. } => format!(
            "{text}: it should finish this many working days after its target date"
        ),
        Zone::None => return None,
    })
}

/// The bar, with its words. `compact` is the small form for a card, a column or a row (bar over
/// words); the full form (in the Schedule section) puts the words first. Nothing for `Zone::None`.
#[component]
pub fn ZoneBar(zone: Zone, #[prop(optional)] compact: bool) -> impl IntoView {
    bar(zone, compact)
}

fn bar(zone: Zone, compact: bool) -> Option<AnyView> {
    let text = zone_text(zone)?;
    let hint = zone_hint(zone).unwrap_or_default();
    let (work, rest) = fill_fractions(zone);
    let tone = zone_tone(zone);
    let (work_class, rest_class) = match zone {
        Zone::AllDone { .. } => ("bg-success/70", ""),
        Zone::Inside {
            level: ZoneLevel::Wide,
            ..
        } => ("bg-muted", "bg-success/70"),
        // No room left: the whole track is the warning.
        Zone::Inside { room: 0, .. } => ("bg-warning/70", "bg-warning/70"),
        Zone::Inside { .. } => ("bg-muted", "bg-warning/70"),
        Zone::Outside { .. } => ("bg-muted", "bg-danger"),
        _ => ("", ""),
    };
    let no_target = matches!(zone, Zone::NoTarget { .. });
    let track = view! {
        <div class=format!(
                 "relative h-1 w-full overflow-hidden rounded-sm bg-hover {}",
                 if no_target { "border-r border-dashed border-line-strong" } else { "" })
             role="img" aria-label=text.clone()>
            <span class=format!("absolute inset-y-0 left-0 {work_class}")
                  style=format!("width:{:.1}%", work * 100.0)></span>
            <span class=format!("absolute inset-y-0 {rest_class}")
                  style=format!("left:{:.1}%;width:{:.1}%", work * 100.0, rest * 100.0)></span>
            {matches!(zone, Zone::Outside { .. }).then(|| view! {
                // The target: where the work should have ended.
                <span class="absolute inset-y-0 w-px bg-fg" style=format!("left:{:.1}%", work * 100.0)></span>
            })}
        </div>
    };
    let words = view! { <span class=format!("shrink-0 tabular-nums {}", tone.text())>{text.clone()}</span> };
    Some(if compact {
        view! {
            <div class="flex w-full min-w-0 flex-col gap-0.5 text-[11px]" title=hint>
                {track}
                {words}
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class="flex w-full min-w-0 flex-col gap-1 text-[12px]" title=hint>
                {words}
                {track}
            </div>
        }
        .into_any()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(remaining: u32, room: u32, level: ZoneLevel) -> Zone {
        Zone::Inside {
            remaining,
            room,
            level,
        }
    }

    #[test]
    fn the_words_say_room_no_room_past_target_or_no_target() {
        assert_eq!(zone_text(Zone::None), None);
        assert_eq!(
            zone_text(inside(3, 6, ZoneLevel::Wide)).as_deref(),
            Some("6 days of room")
        );
        assert_eq!(
            zone_text(inside(3, 1, ZoneLevel::Narrow)).as_deref(),
            Some("1 day of room")
        );
        assert_eq!(
            zone_text(inside(3, 0, ZoneLevel::Closing)).as_deref(),
            Some("No room left")
        );
        assert_eq!(
            zone_text(Zone::Outside {
                remaining: 9,
                over: 3
            })
            .as_deref(),
            Some("3 days past target")
        );
        assert_eq!(
            zone_text(Zone::Outside {
                remaining: 9,
                over: 1
            })
            .as_deref(),
            Some("1 day past target")
        );
        assert_eq!(
            zone_text(Zone::NoTarget { remaining: 4 }).as_deref(),
            Some("No target")
        );
    }

    #[test]
    fn a_project_with_everything_done_says_so_in_green_with_a_full_bar() {
        let done = Zone::AllDone {
            finished: time::macros::date!(2026 - 10 - 08),
        };
        assert_eq!(zone_text(done).as_deref(), Some("All tasks done"));
        assert_eq!(zone_tone(done), Tone::Success);
        assert_eq!(fill_fractions(done), (1.0, 0.0));
        let hint = zone_hint(done).unwrap();
        assert!(
            hint.contains("2026-10-08") && hint.contains("Mark the project done"),
            "{hint}"
        );
    }

    #[test]
    fn the_tone_follows_how_much_room_is_left() {
        assert_eq!(zone_tone(inside(3, 6, ZoneLevel::Wide)), Tone::Success);
        assert_eq!(zone_tone(inside(3, 2, ZoneLevel::Narrow)), Tone::Warning);
        assert_eq!(zone_tone(inside(3, 0, ZoneLevel::Closing)), Tone::Warning);
        assert_eq!(
            zone_tone(Zone::Outside {
                remaining: 4,
                over: 1
            }),
            Tone::Danger
        );
        assert_eq!(zone_tone(Zone::NoTarget { remaining: 4 }), Tone::Neutral);
        assert_eq!(zone_tone(Zone::None), Tone::Neutral);
    }

    #[test]
    fn the_track_splits_into_work_and_room_or_work_and_overflow() {
        assert_eq!(fill_fractions(Zone::None), (0.0, 0.0));
        assert_eq!(fill_fractions(Zone::NoTarget { remaining: 5 }), (0.0, 0.0));
        // 3 days of work and 1 of room: three quarters work.
        assert_eq!(
            fill_fractions(inside(3, 1, ZoneLevel::Narrow)),
            (0.75, 0.25)
        );
        // No room: all work.
        assert_eq!(fill_fractions(inside(4, 0, ZoneLevel::Closing)), (1.0, 0.0));
        assert_eq!(fill_fractions(inside(0, 0, ZoneLevel::Closing)), (0.0, 0.0));
        // 10 days of forecast, 2 past the target: the target tick is at 80%.
        let (within, over) = fill_fractions(Zone::Outside {
            remaining: 10,
            over: 2,
        });
        assert!((within - 0.8).abs() < 1e-9 && (over - 0.2).abs() < 1e-9);
        // More past the target than there is forecast: it is all overflow, never negative.
        let (within, over) = fill_fractions(Zone::Outside {
            remaining: 2,
            over: 5,
        });
        assert_eq!(within, 0.0);
        assert_eq!(over, 1.0);
    }

    #[test]
    fn the_hover_says_what_the_number_is_measured_in() {
        assert_eq!(zone_hint(Zone::None), None);
        let hint = zone_hint(inside(3, 6, ZoneLevel::Wide)).unwrap();
        assert!(
            hint.starts_with("6 days of room: working days between"),
            "{hint}"
        );
        assert!(zone_hint(Zone::NoTarget { remaining: 2 })
            .unwrap()
            .contains("no target date"));
    }
}
