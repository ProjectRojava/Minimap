//! Ongoing objectives (spec 30): no end date, never done, a review rhythm.

use minimap_store::*;
use minimap_types::*;
use time::macros::date;

fn objective(conn: &mut Connection, ongoing: bool, target: Option<Date>) -> Result<Objective> {
    objectives::create(
        conn,
        CreateObjective {
            title: "Internal system maintenance".into(),
            description: String::new(),
            target_date: target,
            status: None,
            priority: None,
            ongoing,
            review_every_days: None,
        },
    )
}

fn update(conn: &mut Connection, id: uuid::Uuid, patch: UpdateObjective) -> Result<Objective> {
    objectives::update(conn, id, patch)
}

#[test]
fn an_ongoing_objective_has_no_target_date_and_is_never_done() {
    let mut conn = open_in_memory().unwrap();
    assert!(matches!(
        objective(&mut conn, true, Some(date!(2027 - 06 - 30))),
        Err(StoreError::Invalid(_))
    ));
    let o = objective(&mut conn, true, None).unwrap();
    assert!(o.ongoing && o.target_date.is_none());
    for patch in [
        UpdateObjective {
            target_date: Patch::Set(date!(2027 - 06 - 30)),
            ..Default::default()
        },
        UpdateObjective {
            status: Some(ObjectiveStatus::Done),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            update(&mut conn, o.id, patch),
            Err(StoreError::Invalid(_))
        ));
    }
    // Nothing changed.
    assert_eq!(
        objectives::get(&conn, o.id).unwrap().status,
        ObjectiveStatus::OnTrack
    );
}

#[test]
fn making_a_goal_ongoing_takes_its_end_away_and_back_again() {
    let mut conn = open_in_memory().unwrap();
    let goal = objective(&mut conn, false, Some(date!(2027 - 06 - 30))).unwrap();
    update(
        &mut conn,
        goal.id,
        UpdateObjective {
            status: Some(ObjectiveStatus::Done),
            ..Default::default()
        },
    )
    .unwrap();
    let o = update(
        &mut conn,
        goal.id,
        UpdateObjective {
            ongoing: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(o.ongoing);
    assert_eq!((o.target_date, o.status), (None, ObjectiveStatus::OnTrack));
    // Back to a goal: it can have a date (and be done) again.
    let g = update(
        &mut conn,
        goal.id,
        UpdateObjective {
            ongoing: Some(false),
            target_date: Patch::Set(date!(2027 - 12 - 31)),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!g.ongoing && g.target_date == Some(date!(2027 - 12 - 31)));
}

#[test]
fn the_review_rhythm_is_one_to_365_days_and_reviewing_is_a_dated_update() {
    let mut conn = open_in_memory().unwrap();
    let o = objective(&mut conn, true, None).unwrap();
    for bad in [0, 366] {
        assert!(update(
            &mut conn,
            o.id,
            UpdateObjective {
                review_every_days: Patch::Set(bad),
                ..Default::default()
            }
        )
        .is_err());
    }
    let o = update(
        &mut conn,
        o.id,
        UpdateObjective {
            review_every_days: Patch::Set(30),
            last_reviewed_on: Patch::Set(date!(2027 - 03 - 01)),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(o.review_due(), Some(date!(2027 - 03 - 31)));
    let o = update(
        &mut conn,
        o.id,
        UpdateObjective {
            review_every_days: Patch::Clear,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(o.review_due(), None);
    assert_eq!(
        objectives::get(&conn, o.id).unwrap().last_reviewed_on,
        Some(date!(2027 - 03 - 01))
    );
}
