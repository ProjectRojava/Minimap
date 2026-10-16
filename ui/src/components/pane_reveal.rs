//! Sections of a task's pane that stay out of the way while they are empty. A section with nothing
//! in it is hidden; the *Add* row offers to start one, and asking for it shows the section (so its
//! own box to type into is there). One of these is provided per open task pane.

use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct Reveal(RwSignal<Vec<&'static str>>);

impl Reveal {
    /// Made by the task pane for as long as it is open.
    pub fn provide() -> Self {
        let r = Self(RwSignal::new(Vec::new()));
        provide_context(r);
        r
    }

    /// The person asked to add to the section named `key`.
    pub fn ask(&self, key: &'static str) {
        self.0.update(|v| {
            if !v.contains(&key) {
                v.push(key);
            }
        });
    }

    pub fn wanted(&self, key: &'static str) -> bool {
        self.0.with(|v| v.contains(&key))
    }
}

/// Whether a section shows: it has something in it, or it was asked for.
pub fn visible(has_content: bool, asked_for: bool) -> bool {
    has_content || asked_for
}

/// [`visible`] for the section `key` of the open pane (reactive: read it inside a closure).
pub fn shown(key: &'static str, has_content: bool) -> bool {
    visible(
        has_content,
        use_context::<Reveal>().is_some_and(|r| r.wanted(key)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_section_shows_when_it_has_content_or_was_asked_for() {
        assert!(visible(true, false));
        assert!(visible(false, true));
        assert!(!visible(false, false));
    }

    #[test]
    fn asking_twice_asks_once() {
        let owner = Owner::new();
        owner.with(|| {
            let r = Reveal::provide();
            assert!(!r.wanted("notes"));
            r.ask("notes");
            r.ask("notes");
            assert!(r.wanted("notes") && !r.wanted("files"));
            assert!(shown("notes", false));
            assert!(!shown("files", false));
        });
    }
}
