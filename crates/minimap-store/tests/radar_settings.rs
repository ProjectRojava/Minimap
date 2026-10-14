//! The radar's settings (spec 53, ADR-0021): about this computer only, validated, and the rings
//! fall back to the defaults when what is stored is no longer valid.

use minimap_store::{meta, open_in_memory, settings};
use minimap_types::{UpdateSettings, DEFAULT_RADAR_RINGS};

#[test]
fn a_new_database_has_the_default_radar_settings() {
    let conn = open_in_memory().unwrap();
    let s = settings::get(&conn).unwrap();
    assert!(s.radar_on_top);
    assert!(!s.radar_open_at_start);
    assert_eq!(s.radar_rings, DEFAULT_RADAR_RINGS);
}

#[test]
fn the_settings_are_saved_and_stay_on_this_device() {
    let mut conn = open_in_memory().unwrap();
    let saved = settings::update(
        &mut conn,
        UpdateSettings {
            radar_on_top: Some(false),
            radar_open_at_start: Some(true),
            radar_rings: Some([3, 8, 40]),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!saved.radar_on_top && saved.radar_open_at_start);
    assert_eq!(saved.radar_rings, [3, 8, 40]);
    assert_eq!(settings::get(&conn).unwrap(), saved);
    for key in ["radar_on_top", "radar_open_at_start", "radar_rings"] {
        assert!(settings::LOCAL_ONLY.contains(&key), "{key} must not sync");
    }
}

#[test]
fn rings_that_do_not_grow_are_refused_and_change_nothing() {
    let mut conn = open_in_memory().unwrap();
    for bad in [[10, 5, 60], [5, 5, 60], [0, 10, 60], [5, 10, 261]] {
        let err = settings::update(
            &mut conn,
            UpdateSettings {
                radar_on_top: Some(false),
                radar_rings: Some(bad),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("ring"), "{err}");
    }
    // Nothing from the refused patches was written, the valid half included.
    let s = settings::get(&conn).unwrap();
    assert!(s.radar_on_top);
    assert_eq!(s.radar_rings, DEFAULT_RADAR_RINGS);
}

#[test]
fn stored_rings_that_are_no_longer_valid_read_back_as_the_defaults() {
    let conn = open_in_memory().unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('radar_rings', '[9, 3, 1]')",
        [],
    )
    .unwrap();
    assert_eq!(
        settings::get(&conn).unwrap().radar_rings,
        DEFAULT_RADAR_RINGS
    );
}

#[test]
fn the_window_place_is_a_device_local_key() {
    let conn = open_in_memory().unwrap();
    assert!(meta::RADAR_GEOMETRY.starts_with(meta::LOCAL_PREFIX));
    meta::set(
        &conn,
        meta::RADAR_GEOMETRY,
        r#"{"x":1,"y":2,"w":260,"h":260}"#,
    )
    .unwrap();
    assert!(meta::get(&conn, meta::RADAR_GEOMETRY).unwrap().is_some());
}
