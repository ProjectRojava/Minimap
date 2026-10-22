use minimap_core::projects::{by_objective, by_status, filter};
use minimap_store::Connection;
use minimap_types::{
    AppError, CreateProject, Date, Project, ProjectArchivePreview, ProjectDetail, ProjectFilter,
    ProjectGroup, ProjectLayout, TaskDisposition, UpdateProject, Uuid,
};
use tauri::State;

use crate::{commands::schedule::zones_at, error::store_error, state::AppState};

/// Active projects, filtered, then arranged as list sections (by objective) or board
/// columns (by status).
#[tauri::command]
pub async fn list_projects(
    state: State<'_, AppState>,
    filter_by: ProjectFilter,
    layout: ProjectLayout,
) -> Result<Vec<ProjectGroup>, AppError> {
    state
        .run(move |conn| list_at(conn, &filter_by, layout, minimap_store::today()))
        .await
}

/// [`list_projects`] as of `today` (the tests fix it). Each row carries its safe zone (spec 44).
pub(crate) fn list_at(
    conn: &Connection,
    filter_by: &ProjectFilter,
    layout: ProjectLayout,
    today: Date,
) -> Result<Vec<ProjectGroup>, AppError> {
    let mut rows = minimap_store::views::project_rows(conn).map_err(store_error)?;
    let zones = zones_at(conn, today)?;
    for row in &mut rows {
        row.zone = zones.get(&row.project.id).copied().unwrap_or_default();
    }
    let rows = filter(rows, filter_by);
    Ok(match layout {
        ProjectLayout::List => by_objective(rows),
        ProjectLayout::Board => by_status(rows),
    })
}

#[tauri::command]
pub async fn get_project(state: State<'_, AppState>, id: Uuid) -> Result<Project, AppError> {
    state
        .run(move |conn| minimap_store::projects::get(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_project_detail(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<ProjectDetail, AppError> {
    state
        .run(move |conn| detail_at(conn, id, minimap_store::today()))
        .await
}

/// [`get_project_detail`] as of `today`, with the project's safe zone (spec 44).
pub(crate) fn detail_at(
    conn: &Connection,
    id: Uuid,
    today: Date,
) -> Result<ProjectDetail, AppError> {
    let mut detail = minimap_store::views::project_detail(conn, id).map_err(store_error)?;
    detail.zone = zones_at(conn, today)?.get(&id).copied().unwrap_or_default();
    Ok(detail)
}

/// The handle is generated from the title unless one is given.
#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    input: CreateProject,
) -> Result<Project, AppError> {
    state
        .run(move |conn| minimap_store::projects::create(conn, input).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn update_project(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateProject,
) -> Result<Project, AppError> {
    state
        .run(move |conn| minimap_store::projects::update(conn, id, patch).map_err(store_error))
        .await
}

/// The active tasks archiving the project would affect; show before `archive_project`.
#[tauri::command]
pub async fn preview_archive_project(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<ProjectArchivePreview, AppError> {
    state
        .run(move |conn| {
            minimap_store::views::project_archive_preview(conn, id).map_err(store_error)
        })
        .await
}

/// Archives the project and its links; its tasks are archived too or moved to the inbox.
#[tauri::command]
pub async fn archive_project(
    state: State<'_, AppState>,
    id: Uuid,
    tasks: TaskDisposition,
) -> Result<(), AppError> {
    state
        .run(move |conn| minimap_store::projects::archive(conn, id, tasks).map_err(store_error))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        AssigneeChoice, CreateProject, CreateTask, ProjectStatus, UpdateProject, UpdateSettings,
        WorkWeek, Zone, ZoneLevel,
    };
    use time::macros::date;

    // Monday 2027-03-01.
    const TODAY: Date = date!(2027 - 03 - 01);

    fn project(
        conn: &mut Connection,
        title: &str,
        target: Option<Date>,
        status: ProjectStatus,
    ) -> Uuid {
        minimap_store::projects::create(
            conn,
            CreateProject {
                title: title.into(),
                slug: None,
                description: String::new(),
                owner_person_id: None,
                start_date: None,
                target_date: target,
                status: Some(status),
                priority: None,
            },
        )
        .unwrap()
        .id
    }

    /// One three-day task: starting today it finishes on Wednesday 2027-03-03.
    fn three_days(conn: &mut Connection, project: Uuid) {
        minimap_store::tasks::create(
            conn,
            CreateTask {
                links: Vec::new(),
                task_type: Some("build".into()),
                focus: None,
                backburner_since: None,
                start_minute: None,
                length_minutes: None,
                title: "Work".into(),
                assignee: AssigneeChoice::Nobody,
                description: String::new(),
                project_id: Some(project),
                status: None,
                estimate_days: Some(3.0),
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap();
    }

    fn zone_of(conn: &Connection, id: Uuid) -> Zone {
        detail_at(conn, id, TODAY).unwrap().zone
    }

    #[test]
    fn every_kind_of_zone_at_a_fixed_date() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let active = ProjectStatus::Active;
        let wide = project(&mut conn, "Wide", Some(date!(2027 - 03 - 10)), active);
        let narrow = project(&mut conn, "Narrow", Some(date!(2027 - 03 - 05)), active);
        let closing = project(&mut conn, "Closing", Some(date!(2027 - 03 - 03)), active);
        let outside = project(&mut conn, "Outside", Some(date!(2027 - 03 - 02)), active);
        let no_target = project(&mut conn, "No target", None, active);
        let paused = project(
            &mut conn,
            "Paused",
            Some(date!(2027 - 03 - 10)),
            ProjectStatus::Paused,
        );
        let empty = project(&mut conn, "Empty", Some(date!(2027 - 03 - 10)), active);
        for p in [wide, narrow, closing, outside, no_target, paused] {
            three_days(&mut conn, p);
        }
        assert_eq!(
            zone_of(&conn, wide),
            Zone::Inside {
                remaining: 3,
                room: 5,
                level: ZoneLevel::Wide
            }
        );
        assert_eq!(
            zone_of(&conn, narrow),
            Zone::Inside {
                remaining: 3,
                room: 2,
                level: ZoneLevel::Narrow
            }
        );
        assert_eq!(
            zone_of(&conn, closing),
            Zone::Inside {
                remaining: 3,
                room: 0,
                level: ZoneLevel::Closing
            }
        );
        assert_eq!(
            zone_of(&conn, outside),
            Zone::Outside {
                remaining: 3,
                over: 1
            }
        );
        assert_eq!(zone_of(&conn, no_target), Zone::NoTarget { remaining: 3 });
        assert_eq!(zone_of(&conn, paused), Zone::None);
        assert_eq!(zone_of(&conn, empty), Zone::None);
    }

    #[test]
    fn a_project_whose_tasks_are_all_done_says_so() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(
            &mut conn,
            "Finished",
            Some(date!(2027 - 03 - 10)),
            ProjectStatus::Active,
        );
        three_days(&mut conn, p);
        let task = minimap_store::tasks::list(&conn, false).unwrap()[0].id;
        minimap_store::tasks::update(
            &mut conn,
            task,
            minimap_types::UpdateTask {
                status: Some(minimap_types::TaskStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(zone_of(&conn, p), Zone::AllDone { .. }));
        // A project with no tasks at all still shows nothing, and closing the project hides it.
        let none = project(&mut conn, "Empty", None, ProjectStatus::Active);
        assert_eq!(zone_of(&conn, none), Zone::None);
        minimap_store::projects::update(
            &mut conn,
            p,
            UpdateProject {
                status: Some(ProjectStatus::Done),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(zone_of(&conn, p), Zone::None);
    }

    #[test]
    fn the_list_and_the_board_rows_carry_the_zone_too() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(
            &mut conn,
            "Wide",
            Some(date!(2027 - 03 - 10)),
            ProjectStatus::Active,
        );
        three_days(&mut conn, p);
        for layout in [ProjectLayout::List, ProjectLayout::Board] {
            let groups = list_at(&conn, &ProjectFilter::default(), layout, TODAY).unwrap();
            let row = groups
                .iter()
                .flat_map(|g| g.rows.iter())
                .find(|r| r.project.id == p)
                .unwrap();
            assert!(
                matches!(row.zone, Zone::Inside { room: 5, .. }),
                "{layout:?}"
            );
        }
    }

    #[test]
    fn the_narrow_setting_and_the_work_week_change_the_room() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(
            &mut conn,
            "Narrow",
            Some(date!(2027 - 03 - 05)),
            ProjectStatus::Active,
        );
        three_days(&mut conn, p);
        assert!(matches!(
            zone_of(&conn, p),
            Zone::Inside {
                level: ZoneLevel::Narrow,
                ..
            }
        ));
        // Narrow at 1 day or fewer: two days of room is wide.
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                zone_narrow_days: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            zone_of(&conn, p),
            Zone::Inside {
                room: 2,
                level: ZoneLevel::Wide,
                ..
            }
        ));
        // Sunday to Thursday: the three days run Monday to Wednesday, but Friday is a day off, so
        // a Friday target leaves only Thursday as room.
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                work_week: Some(WorkWeek::from_days(&[6, 0, 1, 2, 3]).unwrap()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(zone_of(&conn, p), Zone::Inside { room: 1, .. }));
    }

    #[test]
    fn a_paused_or_finished_project_shows_no_zone_and_the_rest_still_do() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = project(
            &mut conn,
            "Wide",
            Some(date!(2027 - 03 - 10)),
            ProjectStatus::Active,
        );
        three_days(&mut conn, p);
        minimap_store::projects::update(
            &mut conn,
            p,
            UpdateProject {
                status: Some(ProjectStatus::Paused),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(zone_of(&conn, p), Zone::None);
    }
}
