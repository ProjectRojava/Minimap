//! Task types (spec 32): rules for the user's list of task types. Pure; the list itself lives in
//! Settings (`store::settings`).

use minimap_types::{TaskType, MAX_TASK_TYPES, MAX_TASK_TYPE_NAME, MEETING_TYPE};

use crate::slug;

/// A list that can be stored: every entry has a name and an id, ids and names are unique (names
/// ignoring case), hues are real hues, and the list is not longer than [`MAX_TASK_TYPES`].
pub fn validate(list: &[TaskType]) -> Result<(), String> {
    if list.len() > MAX_TASK_TYPES {
        return Err(format!("There can be at most {MAX_TASK_TYPES} task types."));
    }
    for (i, t) in list.iter().enumerate() {
        check_name(&t.name)?;
        if t.id.is_empty() {
            return Err("A task type needs an id.".into());
        }
        if t.hue >= 360 {
            return Err(format!(
                "The colour of \"{}\" is not a hue (0-359).",
                t.name
            ));
        }
        for u in &list[i + 1..] {
            if u.id == t.id {
                return Err(format!("Two task types share the id \"{}\".", t.id));
            }
            if u.name.trim().to_lowercase() == t.name.trim().to_lowercase() {
                return Err(format!("There are two task types named \"{}\".", t.name));
            }
        }
    }
    Ok(())
}

fn check_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A task type needs a name.".into());
    }
    if name.chars().count() > MAX_TASK_TYPE_NAME {
        return Err(format!(
            "A task type name can be at most {MAX_TASK_TYPE_NAME} characters."
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("A task type name can't contain control characters.".into());
    }
    Ok(())
}

/// The list the user asked for, made storable: names trimmed, new entries (empty id) given an id
/// made from their name, and checked against the list now stored (`current`): every stored type
/// must still be there (types are archived, never removed, so tasks keep a meaning), and no id
/// may appear that was not handed out.
pub fn normalise(mut wanted: Vec<TaskType>, current: &[TaskType]) -> Result<Vec<TaskType>, String> {
    for t in &mut wanted {
        t.name = t.name.trim().to_owned();
        check_name(&t.name)?;
        // The meeting type is built in: it can be renamed and recoloured, not retired.
        if t.id == MEETING_TYPE {
            t.archived = false;
        }
    }
    for old in current {
        if !wanted.iter().any(|t| t.id == old.id) {
            return Err(format!(
                "The task type \"{}\" can't be removed, only archived.",
                old.name
            ));
        }
    }
    if let Some(t) = wanted
        .iter()
        .find(|t| !t.id.is_empty() && !current.iter().any(|c| c.id == t.id))
    {
        return Err(format!("\"{}\" is not a known task type.", t.name));
    }
    let mut taken: Vec<String> = wanted.iter().map(|t| t.id.clone()).collect();
    for t in &mut wanted {
        if t.id.is_empty() {
            let base = if t.name.chars().any(|c| c.is_ascii_alphanumeric()) {
                slug::slugify(&t.name)
            } else {
                "type".to_owned()
            };
            t.id = slug::unique(&base, |c| taken.iter().any(|x| x == c));
            taken.push(t.id.clone());
        }
    }
    validate(&wanted)?;
    Ok(wanted)
}

/// The list with the built-in meeting type in it (spec 38): added when missing (named "Meeting",
/// or "Meeting (built-in)" if another type already has that name), and brought back if it was
/// archived. Its name and colour are the user's to change.
pub fn ensure_meeting(mut list: Vec<TaskType>) -> Vec<TaskType> {
    if let Some(t) = list.iter_mut().find(|t| t.id == MEETING_TYPE) {
        t.archived = false;
        return list;
    }
    let taken = |name: &str| {
        list.iter()
            .any(|t| t.name.trim().to_lowercase() == name.to_lowercase())
    };
    let name = if taken("Meeting") {
        "Meeting (built-in)"
    } else {
        "Meeting"
    };
    list.push(TaskType {
        id: MEETING_TYPE.to_owned(),
        name: name.to_owned(),
        hue: 75,
        archived: false,
    });
    list
}

/// The active type a typed word means: its id or its name, ignoring case (`type:decision`,
/// `type:"Legal review"`).
pub fn find_active<'a>(types: &'a [TaskType], text: &str) -> Option<&'a TaskType> {
    let text = text.trim().to_lowercase();
    types
        .iter()
        .filter(|t| !t.archived)
        .find(|t| t.id == text || t.name.to_lowercase() == text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::default_task_types;

    fn new_type(name: &str) -> TaskType {
        TaskType {
            id: String::new(),
            name: name.into(),
            hue: 215,
            archived: false,
        }
    }

    #[test]
    fn the_defaults_are_a_valid_list() {
        assert_eq!(validate(&default_task_types()), Ok(()));
    }

    #[test]
    fn a_new_type_gets_an_id_from_its_name() {
        let current = default_task_types();
        let mut wanted = current.clone();
        wanted.push(new_type("  Legal review "));
        wanted.push(new_type("Design")); // same as an existing name
        assert!(normalise(wanted.clone(), &current)
            .unwrap_err()
            .contains("two task types named"));
        wanted.pop();
        let out = normalise(wanted, &current).unwrap();
        let added = out.last().unwrap();
        assert_eq!(
            (added.id.as_str(), added.name.as_str()),
            ("legal-review", "Legal review")
        );
    }

    #[test]
    fn ids_are_unique_and_names_without_letters_still_get_one() {
        let current = default_task_types();
        let mut wanted = current.clone();
        wanted.push(new_type("Bug!")); // slug "bug" is taken
        wanted.push(new_type("设计"));
        let out = normalise(wanted, &current).unwrap();
        assert_eq!(out[8].id, "bug-2");
        assert_eq!(out[9].id, "type");
    }

    #[test]
    fn the_meeting_type_is_always_there_and_never_retired() {
        // Missing: added with its own colour.
        let without: Vec<TaskType> = default_task_types()
            .into_iter()
            .filter(|t| t.id != MEETING_TYPE)
            .collect();
        let with = ensure_meeting(without.clone());
        assert_eq!(with.len(), without.len() + 1);
        assert_eq!(
            with.last().map(|t| (t.id.as_str(), t.name.as_str())),
            Some((MEETING_TYPE, "Meeting"))
        );
        assert_eq!(validate(&with), Ok(()));
        // Present: left alone, but brought back if it was archived.
        let mut archived = default_task_types();
        archived
            .iter_mut()
            .find(|t| t.id == MEETING_TYPE)
            .unwrap()
            .archived = true;
        let back = ensure_meeting(archived.clone());
        assert!(back.iter().all(|t| !t.archived));
        // A name that is taken: the built-in one is told apart.
        let mut clash = without;
        clash.push(TaskType {
            id: "meetings".into(),
            name: "meeting".into(),
            hue: 10,
            archived: false,
        });
        let out = ensure_meeting(clash);
        assert_eq!(out.last().unwrap().name, "Meeting (built-in)");
        assert_eq!(validate(&out), Ok(()));
        // Archiving it through an edit does not stick; renaming does.
        let current = default_task_types();
        let mut wanted = current.clone();
        let m = wanted.iter_mut().find(|t| t.id == MEETING_TYPE).unwrap();
        m.archived = true;
        m.name = "Call".into();
        let out = normalise(wanted, &current).unwrap();
        let m = out.iter().find(|t| t.id == MEETING_TYPE).unwrap();
        assert!(!m.archived);
        assert_eq!(m.name, "Call");
    }

    #[test]
    fn a_rename_keeps_the_id_and_an_archive_keeps_the_type() {
        let current = default_task_types();
        let mut wanted = current.clone();
        wanted[0].name = "UX design".into();
        wanted[1].archived = true;
        let out = normalise(wanted, &current).unwrap();
        assert_eq!(out[0].id, "design");
        assert_eq!(out[0].name, "UX design");
        assert!(out[1].archived);
    }

    #[test]
    fn a_type_cannot_be_removed_or_invented() {
        let current = default_task_types();
        let mut fewer = current.clone();
        fewer.pop();
        assert!(normalise(fewer, &current)
            .unwrap_err()
            .contains("only archived"));
        let mut invented = current.clone();
        invented.push(TaskType {
            id: "ghost".into(),
            ..new_type("Ghost")
        });
        assert!(normalise(invented, &current)
            .unwrap_err()
            .contains("not a known"));
    }

    #[test]
    fn bad_names_hues_and_lengths_are_refused() {
        let current = default_task_types();
        for bad in ["", "   ", &"x".repeat(31)] {
            let mut wanted = current.clone();
            wanted.push(new_type(bad));
            assert!(normalise(wanted, &current).is_err(), "{bad:?}");
        }
        let mut wanted = current.clone();
        wanted[0].hue = 400;
        assert!(normalise(wanted, &current).unwrap_err().contains("hue"));
        let mut many = current.clone();
        for i in 0..MAX_TASK_TYPES {
            many.push(new_type(&format!("Type {i}")));
        }
        assert!(normalise(many, &current).unwrap_err().contains("at most"));
    }

    #[test]
    fn a_word_finds_a_type_by_id_or_name_unless_it_is_archived() {
        let mut types = default_task_types();
        assert_eq!(find_active(&types, "decision").unwrap().id, "decision");
        assert_eq!(find_active(&types, " DESIGN ").unwrap().id, "design");
        types[0].archived = true;
        assert!(find_active(&types, "design").is_none());
        assert!(find_active(&types, "unknown").is_none());
    }
}
