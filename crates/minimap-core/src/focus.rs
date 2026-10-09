//! Focus (spec 37): turning what the user picked into what is stored. Pure. (Reading a stored
//! focus back as a choice is `FocusChoice::of`, in the types, so the screen can use it.)

use minimap_types::{Date, Focus, FocusChoice};

/// The focus a choice means on `today`: `None` takes the task out of focus. "Today" is a focus
/// that ends today; a date that has passed is refused, since it would put nothing in focus.
pub fn resolve(choice: FocusChoice, today: Date) -> Result<Option<Focus>, String> {
    match choice {
        FocusChoice::Off => Ok(None),
        FocusChoice::Pinned => Ok(Some(Focus::PINNED)),
        FocusChoice::Today => Ok(Some(Focus { until: Some(today) })),
        FocusChoice::Until { date } if date < today => Err(format!(
            "{date} has already passed. Pick today or a later day to keep the task in focus."
        )),
        FocusChoice::Until { date } => Ok(Some(Focus { until: Some(date) })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    const TODAY: Date = date!(2027 - 03 - 03);

    #[test]
    fn each_choice_becomes_its_focus() {
        assert_eq!(resolve(FocusChoice::Off, TODAY), Ok(None));
        assert_eq!(resolve(FocusChoice::Pinned, TODAY), Ok(Some(Focus::PINNED)));
        assert_eq!(
            resolve(FocusChoice::Today, TODAY),
            Ok(Some(Focus { until: Some(TODAY) }))
        );
        let later = date!(2027 - 04 - 01);
        assert_eq!(
            resolve(FocusChoice::Until { date: later }, TODAY),
            Ok(Some(Focus { until: Some(later) }))
        );
        // Today is allowed as the last day.
        assert!(resolve(FocusChoice::Until { date: TODAY }, TODAY).is_ok());
    }

    #[test]
    fn a_day_that_has_passed_is_refused_with_a_reason() {
        let err = resolve(
            FocusChoice::Until {
                date: date!(2027 - 03 - 02),
            },
            TODAY,
        )
        .unwrap_err();
        assert!(
            err.contains("2027-03-02") && err.contains("passed"),
            "{err}"
        );
    }

    #[test]
    fn a_stored_focus_reads_back_as_the_choice_that_made_it() {
        for choice in [
            FocusChoice::Off,
            FocusChoice::Pinned,
            FocusChoice::Today,
            FocusChoice::Until {
                date: date!(2027 - 04 - 01),
            },
        ] {
            let focus = resolve(choice, TODAY).unwrap();
            assert_eq!(FocusChoice::of(focus.as_ref(), TODAY), choice);
        }
    }
}
