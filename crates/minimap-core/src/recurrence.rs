//! Recurring items (spec 27): the dates and text of a repeat rule. Pure.
//!
//! - A **task** repeats on completion: finishing it makes the next one, due on the rule's next
//!   date (`next_for_task`). The schedule stays anchored to the rule, not to when you got round
//!   to it: finishing a weekly Monday task on Wednesday gives next Monday, and one finished weeks
//!   late gives the next Monday that is not in the past.
//! - A **note** repeats on its date: when the next date has come, a new note is made
//!   (`due_note_date`), starting from the rule's template and the open checklist items of the
//!   last one (`new_note_body`). Only the latest due date is made, never one per missed week.
//!
//! Rules: `day`, `mon`..`sun` (every week on that day), `week`, `2w` / `2w:mon` (every N weeks),
//! `month` / `month:15` (a day of the month, the last day in shorter months). `week`, `Nw` and
//! `month` without a day take it from an anchor date (the due date).

use minimap_types::{Cadence, Date, Recurrence};
use time::{Duration, Month};

use crate::notes::checklist;

/// The longest template for a repeating note.
pub const MAX_TEMPLATE_CHARS: usize = 20_000;

const HELP: &str =
    "Repeats look like day, mon, 2w, 2w:fri or month (month:15 for a day of the month)";

pub fn validate(r: &Recurrence) -> Result<(), String> {
    match r.cadence {
        Cadence::Daily => {}
        Cadence::Weekly { every, weekday } => {
            if !(1..=52).contains(&every) {
                return Err("A repeat of N weeks needs N from 1 to 52".into());
            }
            if weekday > 6 {
                return Err("A weekday is a number from 0 (Monday) to 6 (Sunday)".into());
            }
        }
        Cadence::Monthly { day } => {
            if !(1..=31).contains(&day) {
                return Err("A day of the month is from 1 to 31".into());
            }
        }
    }
    if r.template
        .as_ref()
        .is_some_and(|t| t.chars().count() > MAX_TEMPLATE_CHARS)
    {
        return Err(format!(
            "The template is longer than {MAX_TEMPLATE_CHARS} characters"
        ));
    }
    Ok(())
}

fn weekday_number(word: &str) -> Option<u8> {
    Some(match word {
        "mon" | "monday" => 0,
        "tue" | "tues" | "tuesday" => 1,
        "wed" | "wednesday" => 2,
        "thu" | "thur" | "thurs" | "thursday" => 3,
        "fri" | "friday" => 4,
        "sat" | "saturday" => 5,
        "sun" | "sunday" => 6,
        _ => return None,
    })
}

/// Reads a repeat rule: what follows `every:` in quick-add, or what is typed in the Repeats
/// box. `anchor` (the due date, else today) supplies the weekday or day of the month when the
/// text doesn't.
pub fn parse_every(text: &str, anchor: Date) -> Result<Cadence, String> {
    let text = text.trim().to_lowercase();
    let anchor_weekday = anchor.weekday().number_days_from_monday();
    let (head, tail) = match text.split_once(':') {
        Some((h, t)) => (h.trim(), Some(t.trim())),
        None => (text.as_str(), None),
    };
    if let Some(wd) = weekday_number(head) {
        return match tail {
            None => Ok(Cadence::Weekly {
                every: 1,
                weekday: wd,
            }),
            Some(_) => Err(HELP.into()),
        };
    }
    match head {
        "day" | "daily" if tail.is_none() => return Ok(Cadence::Daily),
        "week" | "weekly" => {
            let weekday = match tail {
                None => anchor_weekday,
                Some(t) => weekday_number(t).ok_or_else(|| HELP.to_owned())?,
            };
            return Ok(Cadence::Weekly { every: 1, weekday });
        }
        "month" | "monthly" => {
            let day = match tail {
                None => anchor.day(),
                Some(t) => t
                    .trim_end_matches(|c: char| c.is_alphabetic())
                    .parse::<u8>()
                    .map_err(|_| HELP.to_owned())?,
            };
            if !(1..=31).contains(&day) {
                return Err("A day of the month is from 1 to 31".into());
            }
            return Ok(Cadence::Monthly { day });
        }
        _ => {}
    }
    // 2w, 2w:fri
    if let Some(n) = head.strip_suffix('w').filter(|n| !n.is_empty()) {
        let every: u32 = n.parse().map_err(|_| HELP.to_owned())?;
        if !(1..=52).contains(&every) {
            return Err("A repeat of N weeks needs N from 1 to 52".into());
        }
        let weekday = match tail {
            None => anchor_weekday,
            Some(t) => weekday_number(t).ok_or_else(|| HELP.to_owned())?,
        };
        return Ok(Cadence::Weekly { every, weekday });
    }
    Err(HELP.into())
}

fn plus_days(d: Date, n: i64) -> Date {
    d.checked_add(Duration::days(n)).unwrap_or(Date::MAX)
}

/// The date in `year`/`month` that `day` means (the last day if the month is shorter).
fn month_day(year: i32, month: Month, day: u8) -> Option<Date> {
    Date::from_calendar_date(year, month, day.min(month.length(year))).ok()
}

/// The first date on or after `d` that is on day `day` of a month.
fn monthly_on_or_after(day: u8, d: Date) -> Date {
    if let Some(here) = month_day(d.year(), d.month(), day).filter(|c| *c >= d) {
        return here;
    }
    let (year, month) = match d.month() {
        Month::December => (d.year() + 1, Month::January),
        m => (d.year(), m.next()),
    };
    month_day(year, month, day).unwrap_or(Date::MAX)
}

/// The first date the rule gives after `d`.
///
/// Every N weeks counts from the first matching weekday after `d`, so a date that is itself on
/// the weekday gives `d` + N weeks.
pub fn next_after(c: Cadence, d: Date) -> Date {
    match c {
        Cadence::Daily => plus_days(d, 1),
        Cadence::Weekly { every, weekday } => {
            let ahead =
                (i64::from(weekday) + 7 - i64::from(d.weekday().number_days_from_monday())) % 7;
            let first = plus_days(d, if ahead == 0 { 7 } else { ahead });
            plus_days(first, 7 * (i64::from(every.max(1)) - 1))
        }
        Cadence::Monthly { day } => monthly_on_or_after(day, plus_days(d, 1)),
    }
}

/// The first date on or after `d` that the rule falls on (where a new repeating item starts).
pub fn first_on_or_after(c: Cadence, d: Date) -> Date {
    match c {
        Cadence::Daily => d,
        Cadence::Weekly { weekday, .. } => {
            let ahead =
                (i64::from(weekday) + 7 - i64::from(d.weekday().number_days_from_monday())) % 7;
            plus_days(d, ahead)
        }
        Cadence::Monthly { day } => monthly_on_or_after(day, d),
    }
}

/// When the task after one that was just finished is due: the rule's next date after its due
/// date (or after today when it has none), never in the past.
pub fn next_for_task(c: Cadence, due: Option<Date>, today: Date) -> Date {
    let from = due.unwrap_or(today).max(plus_days(today, -1));
    next_after(c, from)
}

/// The date of the note to make now for a repeating note dated `note_date`: the latest date the
/// rule gives that is after it and not after `today`. `None` while the next date hasn't come.
pub fn due_note_date(c: Cadence, note_date: Date, today: Date) -> Option<Date> {
    let mut due = next_after(c, note_date);
    if due > today {
        return None;
    }
    for _ in 0..100_000 {
        let next = next_after(c, due);
        if next > today || next <= due {
            break;
        }
        due = next;
    }
    Some(due)
}

/// What a new note of a repeating series starts with: the rule's template, then the previous
/// note's open checklist items under "Carried over".
pub fn new_note_body(template: Option<&str>, previous_body: &str) -> String {
    let lines: Vec<&str> = previous_body.split('\n').collect();
    let carried: Vec<&str> = checklist(previous_body)
        .iter()
        .filter_map(|item| lines.get(item.line as usize).copied())
        .map(str::trim_end)
        .collect();
    let mut out = template.unwrap_or("").trim_end().to_owned();
    if !carried.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("## Carried over\n\n");
        out.push_str(&carried.join("\n"));
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use time::macros::date;

    // 2027-03-03 is a Wednesday.
    const WED: Date = date!(2027 - 03 - 03);

    fn weekly(every: u32, weekday: u8) -> Cadence {
        Cadence::Weekly { every, weekday }
    }

    #[test]
    fn rules_are_read_from_short_text() {
        let p = |t: &str| parse_every(t, WED);
        assert_eq!(p("day"), Ok(Cadence::Daily));
        assert_eq!(p("Daily"), Ok(Cadence::Daily));
        assert_eq!(p("mon"), Ok(weekly(1, 0)));
        assert_eq!(p("Friday"), Ok(weekly(1, 4)));
        assert_eq!(p("sun"), Ok(weekly(1, 6)));
        // Without a day, the anchor's: a Wednesday.
        assert_eq!(p("week"), Ok(weekly(1, 2)));
        assert_eq!(p("2w"), Ok(weekly(2, 2)));
        assert_eq!(p("2w:fri"), Ok(weekly(2, 4)));
        assert_eq!(p("week:mon"), Ok(weekly(1, 0)));
        assert_eq!(p("month"), Ok(Cadence::Monthly { day: 3 }));
        assert_eq!(p("monthly"), Ok(Cadence::Monthly { day: 3 }));
        assert_eq!(p("month:15"), Ok(Cadence::Monthly { day: 15 }));
        assert_eq!(p("month:15th"), Ok(Cadence::Monthly { day: 15 }));
        assert_eq!(p(" 12w "), Ok(weekly(12, 2)));
    }

    #[test]
    fn what_is_not_a_rule_says_what_is() {
        for bad in [
            "", "soon", "0w", "53w", "w", "2x", "month:0", "month:32", "month:x", "mon:tue",
            "day:mon", "2w:xyz",
        ] {
            let e = parse_every(bad, WED).unwrap_err();
            assert!(
                e.contains("Repeats look like") || e.contains("from 1 to"),
                "{bad}: {e}"
            );
        }
    }

    #[test]
    fn a_shorthand_always_reads_back_as_the_same_rule() {
        for c in [
            Cadence::Daily,
            weekly(1, 0),
            weekly(1, 6),
            weekly(2, 3),
            weekly(52, 5),
            Cadence::Monthly { day: 1 },
            Cadence::Monthly { day: 31 },
        ] {
            // Whatever the anchor is.
            for anchor in [WED, date!(2027 - 12 - 31), date!(2028 - 02 - 29)] {
                assert_eq!(parse_every(&c.shorthand(), anchor), Ok(c), "{c:?}");
            }
        }
    }

    #[test]
    fn the_next_date_after_a_date() {
        // Daily.
        assert_eq!(next_after(Cadence::Daily, WED), date!(2027 - 03 - 04));
        assert_eq!(
            next_after(Cadence::Daily, date!(2027 - 12 - 31)),
            date!(2028 - 01 - 01)
        );
        // Weekly on Monday, from a Wednesday and from a Monday.
        assert_eq!(next_after(weekly(1, 0), WED), date!(2027 - 03 - 08));
        assert_eq!(
            next_after(weekly(1, 0), date!(2027 - 03 - 01)),
            date!(2027 - 03 - 08)
        );
        // Every 2 weeks on Monday: from a Monday it is 14 days, from a Wednesday the Monday after
        // next.
        assert_eq!(
            next_after(weekly(2, 0), date!(2027 - 03 - 01)),
            date!(2027 - 03 - 15)
        );
        assert_eq!(next_after(weekly(2, 0), WED), date!(2027 - 03 - 15));
        // Monthly.
        let on = |day| Cadence::Monthly { day };
        assert_eq!(next_after(on(15), WED), date!(2027 - 03 - 15));
        assert_eq!(
            next_after(on(15), date!(2027 - 03 - 15)),
            date!(2027 - 04 - 15)
        );
        assert_eq!(next_after(on(3), WED), date!(2027 - 04 - 03));
        assert_eq!(
            next_after(on(1), date!(2027 - 12 - 15)),
            date!(2028 - 01 - 01)
        );
        // Short months use their last day, and the day comes back after.
        assert_eq!(
            next_after(on(31), date!(2027 - 01 - 31)),
            date!(2027 - 02 - 28)
        );
        assert_eq!(
            next_after(on(31), date!(2027 - 02 - 28)),
            date!(2027 - 03 - 31)
        );
        assert_eq!(
            next_after(on(30), date!(2028 - 01 - 30)),
            date!(2028 - 02 - 29)
        );
    }

    #[test]
    fn where_a_new_repeating_item_starts() {
        assert_eq!(first_on_or_after(Cadence::Daily, WED), WED);
        // A Wednesday is already Wednesday; a Monday rule waits for the next Monday.
        assert_eq!(first_on_or_after(weekly(1, 2), WED), WED);
        assert_eq!(first_on_or_after(weekly(3, 0), WED), date!(2027 - 03 - 08));
        assert_eq!(first_on_or_after(Cadence::Monthly { day: 3 }, WED), WED);
        assert_eq!(
            first_on_or_after(Cadence::Monthly { day: 2 }, WED),
            date!(2027 - 04 - 02)
        );
    }

    #[test]
    fn finishing_a_task_gives_the_next_date_that_is_not_in_the_past() {
        let mon = weekly(1, 0);
        // On time (due Monday 1st, finished that day): next Monday.
        assert_eq!(
            next_for_task(mon, Some(date!(2027 - 03 - 01)), date!(2027 - 03 - 01)),
            date!(2027 - 03 - 08)
        );
        // A bit late (finished on Wednesday): still next Monday, not the one that passed.
        assert_eq!(
            next_for_task(mon, Some(date!(2027 - 03 - 01)), WED),
            date!(2027 - 03 - 08)
        );
        // Early: the due date is the anchor.
        assert_eq!(
            next_for_task(mon, Some(date!(2027 - 03 - 08)), date!(2027 - 03 - 01)),
            date!(2027 - 03 - 15)
        );
        // Weeks late: the first Monday after today, not a pile of past ones.
        assert_eq!(
            next_for_task(mon, Some(date!(2027 - 01 - 04)), WED),
            date!(2027 - 03 - 08)
        );
        // Late and today is a Monday: today counts.
        assert_eq!(
            next_for_task(mon, Some(date!(2027 - 01 - 04)), date!(2027 - 03 - 08)),
            date!(2027 - 03 - 08)
        );
        // Daily, due today, finished today: tomorrow.
        assert_eq!(
            next_for_task(Cadence::Daily, Some(WED), WED),
            date!(2027 - 03 - 04)
        );
        // No due date: counted from today.
        assert_eq!(next_for_task(mon, None, WED), date!(2027 - 03 - 08));
        assert_eq!(
            next_for_task(Cadence::Monthly { day: 1 }, None, WED),
            date!(2027 - 04 - 01)
        );
    }

    #[test]
    fn a_note_is_made_on_its_date_and_only_the_latest_one() {
        let mon = weekly(1, 0);
        let note = date!(2027 - 03 - 01);
        // Until the next Monday there is nothing to make.
        assert_eq!(due_note_date(mon, note, WED), None);
        assert_eq!(due_note_date(mon, note, date!(2027 - 03 - 07)), None);
        // On the day, and after.
        assert_eq!(
            due_note_date(mon, note, date!(2027 - 03 - 08)),
            Some(date!(2027 - 03 - 08))
        );
        assert_eq!(
            due_note_date(mon, note, date!(2027 - 03 - 11)),
            Some(date!(2027 - 03 - 08))
        );
        // Away for a month: one note, for the latest Monday that has come.
        assert_eq!(
            due_note_date(mon, note, date!(2027 - 04 - 14)),
            Some(date!(2027 - 04 - 12))
        );
        assert_eq!(
            due_note_date(Cadence::Daily, note, date!(2027 - 03 - 20)),
            Some(date!(2027 - 03 - 20))
        );
    }

    #[test]
    fn a_new_note_starts_from_the_template_and_the_open_items() {
        let previous = "Talked.\n\n- [x] Done thing\n- [ ] Ask @[Raj](node:00000000-0000-0000-0000-000000000001) about it\n  - [ ] A nested one\n* [ ] Star bullet\n";
        assert_eq!(
            new_note_body(Some("## Agenda\n\n- \n"), previous),
            "## Agenda\n\n-\n\n## Carried over\n\n- [ ] Ask @[Raj](node:00000000-0000-0000-0000-000000000001) about it\n  - [ ] A nested one\n* [ ] Star bullet\n"
        );
        // Only the template, only the items, or nothing.
        assert_eq!(
            new_note_body(Some("## Agenda"), "no list here"),
            "## Agenda\n"
        );
        assert_eq!(
            new_note_body(None, "- [ ] one\n- [ ] two"),
            "## Carried over\n\n- [ ] one\n- [ ] two\n"
        );
        assert_eq!(new_note_body(None, "nothing"), "");
        assert_eq!(new_note_body(Some("   "), "- [x] done"), "");
    }

    #[test]
    fn rules_are_checked_before_they_are_kept() {
        assert!(validate(&Cadence::Daily.into()).is_ok());
        assert!(validate(&weekly(0, 0).into()).is_err());
        assert!(validate(&weekly(53, 0).into()).is_err());
        assert!(validate(&weekly(1, 7).into()).is_err());
        assert!(validate(&Cadence::Monthly { day: 0 }.into()).is_err());
        assert!(validate(&Cadence::Monthly { day: 32 }.into()).is_err());
        let long = Recurrence {
            cadence: Cadence::Daily,
            template: Some("x".repeat(MAX_TEMPLATE_CHARS + 1)),
        };
        assert!(validate(&long).is_err());
    }

    fn arb_cadence() -> impl Strategy<Value = Cadence> {
        prop_oneof![
            Just(Cadence::Daily),
            (1u32..=52, 0u8..=6).prop_map(|(every, weekday)| Cadence::Weekly { every, weekday }),
            (1u8..=31).prop_map(|day| Cadence::Monthly { day }),
        ]
    }

    fn arb_date() -> impl Strategy<Value = Date> {
        (-20_000i64..30_000).prop_map(|n| date!(2000 - 01 - 01) + Duration::days(n))
    }

    proptest! {
        /// The next date is always after the date, never skips an occurrence of a daily rule, and
        /// lands on the rule's weekday or day.
        #[test]
        fn the_next_date_is_after_and_on_the_rule(c in arb_cadence(), d in arb_date()) {
            let next = next_after(c, d);
            prop_assert!(next > d);
            match c {
                Cadence::Daily => prop_assert_eq!(next, d + Duration::days(1)),
                Cadence::Weekly { every, weekday } => {
                    prop_assert_eq!(next.weekday().number_days_from_monday(), weekday);
                    let gap = (next - d).whole_days();
                    prop_assert!(gap > i64::from(every - 1) * 7 && gap <= i64::from(every) * 7);
                }
                Cadence::Monthly { day } => {
                    let last = next.month().length(next.year());
                    prop_assert_eq!(next.day(), day.min(last));
                    prop_assert!((next - d).whole_days() <= 31);
                }
            }
        }

        /// Repeated again and again the dates keep rising and keep to the rule.
        #[test]
        fn a_series_keeps_rising(c in arb_cadence(), d in arb_date()) {
            let mut at = d;
            for _ in 0..30 {
                let next = next_after(c, at);
                prop_assert!(next > at);
                at = next;
            }
        }

        /// The first date on or after `d` is `d` or later and is on the rule's weekday or day.
        #[test]
        fn the_first_date_is_not_before(c in arb_cadence(), d in arb_date()) {
            let first = first_on_or_after(c, d);
            prop_assert!(first >= d);
            prop_assert!((first - d).whole_days() <= 31);
            if let Cadence::Weekly { weekday, .. } = c {
                prop_assert_eq!(first.weekday().number_days_from_monday(), weekday);
            }
        }

        /// A finished task's next date is after its due date and not in the past.
        #[test]
        fn the_next_task_is_in_the_future_and_after_its_due_date(
            c in arb_cadence(), due in arb_date(), today in arb_date()
        ) {
            let next = next_for_task(c, Some(due), today);
            prop_assert!(next > due);
            prop_assert!(next >= today);
        }

        /// A note is made only once its date has come, and the date is on the rule.
        #[test]
        fn a_note_is_never_made_early_or_twice_for_the_same_date(
            c in arb_cadence(), note in arb_date(), today in arb_date()
        ) {
            match due_note_date(c, note, today) {
                None => prop_assert!(next_after(c, note) > today),
                Some(d) => {
                    prop_assert!(d > note && d <= today);
                    // Nothing later has come.
                    prop_assert!(next_after(c, d) > today);
                    // And it is a date of the rule: the series from the note reaches it.
                    let mut at = note;
                    let mut found = false;
                    for _ in 0..100_000 {
                        at = next_after(c, at);
                        if at == d { found = true; break; }
                        if at > d { break; }
                    }
                    prop_assert!(found);
                }
            }
        }
    }
}
