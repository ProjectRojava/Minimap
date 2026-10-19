//! A person's avatar: two letters inside a shape, in the person's colour.
//!
//! The colour is the person's hue (by id, creation order: `objective_colour::assign`). The shape is
//! *derived from that hue*, so neighbours in the palette differ in both: two people whose names
//! start the same way look different, and the letters say which is which. Colour and shape are never
//! the only signal; the person's name is the tooltip.

use std::collections::HashMap;

use leptos::prelude::*;
use minimap_types::{NodeRef, NodeType, Uuid};

use super::objective_colour::{assign, PALETTE};
use crate::{api, state::DataVersion};

/// The shapes, in the order they are handed out. All are drawn in a 32 x 32 box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Squircle,
    Hexagon,
    Octagon,
}

impl Shape {
    /// The outline as an SVG path in a 32 x 32 box.
    pub fn path(self) -> &'static str {
        match self {
            Shape::Circle => "M16 1.5a14.5 14.5 0 1 0 0 29a14.5 14.5 0 1 0 0-29Z",
            Shape::Squircle => {
                "M9 2.5h14a6.5 6.5 0 0 1 6.5 6.5v14a6.5 6.5 0 0 1-6.5 6.5H9A6.5 6.5 0 0 1 2.5 23V9A6.5 6.5 0 0 1 9 2.5Z"
            }
            Shape::Hexagon => "M16 1.5 29 8.75v14.5L16 30.5 3 23.25V8.75Z",
            Shape::Octagon => "M10.5 2.5h11L29.5 10.5v11L21.5 29.5h-11L2.5 21.5v-11Z",
        }
    }
}

/// The shape that goes with a hue: it follows the hue's place in `objective_colour::PALETTE`, so
/// the first four people have four shapes and the next four repeat them in other colours.
pub fn shape_for(hue: u16) -> Shape {
    let n = PALETTE
        .iter()
        .position(|h| *h == hue)
        .unwrap_or(hue as usize);
    match n % 4 {
        0 => Shape::Circle,
        1 => Shape::Squircle,
        2 => Shape::Hexagon,
        _ => Shape::Octagon,
    }
}

/// The avatar's letters, in capitals. Several words: the first letter of the first and of the
/// last ("Priya Nair" is "PN", "Mary Jane Watson" is "MW"). One word: its first two letters
/// ("Pawan" is "PA", "Prayrit" is "PR"). Nothing for an empty name.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let letters: Vec<char> = match words.as_slice() {
        [] => Vec::new(),
        [one] => one.chars().take(2).collect(),
        [first, .., last] => first.chars().take(1).chain(last.chars().take(1)).collect(),
    };
    letters.into_iter().flat_map(char::to_uppercase).collect()
}

/// A person's avatar at `size` pixels. `hue` is `None` while the people are still loading (a plain
/// grey avatar). Give it a `title` when it is the only place the name shows.
#[component]
pub fn Avatar(
    #[prop(into)] name: String,
    #[prop(into)] hue: Signal<Option<u16>>,
    #[prop(default = 22)] size: u32,
    #[prop(optional, into)] title: Option<String>,
) -> impl IntoView {
    let letters = initials(&name);
    // One letter is a little bigger than two, which have to fit the narrowest shape.
    let font = if letters.chars().count() > 1 { 13 } else { 16 };
    let shape = move || hue.get().map(shape_for).unwrap_or(Shape::Circle).path();
    let style = move || hue.get().map(|h| format!("--obj-h: {h}"));
    let class = move || {
        if hue.get().is_some() {
            "avatar-svg"
        } else {
            "avatar-svg plain"
        }
    };
    let labelled = title.is_some();
    view! {
        <svg class=class style=style width=size height=size viewBox="0 0 32 32"
             role=labelled.then_some("img") aria-label=title.clone()
             aria-hidden=(!labelled).then_some("true")>
            {title.clone().map(|t| view! { <title>{t}</title> })}
            <path d=shape />
            <text x="16" y="16" dy="0.35em" font-size=font>{letters}</text>
        </svg>
    }
}

/// Everyone's colour and name, from the shell: ranked by id like objectives, so a person looks the
/// same on every screen and never changes under a filter. Provided once by the shell.
#[derive(Clone, Copy)]
pub struct PeopleLooks {
    by_id: Memo<HashMap<Uuid, (u16, String)>>,
}

impl PeopleLooks {
    /// Loads the people (again whenever data changes) and provides the result as a context.
    pub fn provide() -> Self {
        let version = expect_context::<DataVersion>();
        let people = LocalResource::new(move || {
            version.track();
            api::list_node_summaries(NodeType::Person)
        });
        // Keep the last answer while reloading, so avatars never blink out after an edit.
        let by_id =
            Memo::new(
                move |last: Option<&HashMap<Uuid, (u16, String)>>| match people.get() {
                    Some(Ok(list)) => {
                        let hues = assign(&list.iter().map(|p| p.node.id).collect::<Vec<_>>());
                        list.into_iter()
                            .filter_map(|p| {
                                hues.get(&p.node.id).map(|h| (p.node.id, (*h, p.label)))
                            })
                            .collect()
                    }
                    _ => last.cloned().unwrap_or_default(),
                },
            );
        let looks = Self { by_id };
        provide_context(looks);
        looks
    }

    /// A person's hue and name (tracks, so call it inside a closure that redraws).
    pub fn look(&self, id: Uuid) -> Option<(u16, String)> {
        self.by_id.with(|m| m.get(&id).cloned())
    }

    /// Whether `id` is a person we know.
    pub fn knows(&self, id: Uuid) -> bool {
        self.by_id.with(|m| m.contains_key(&id))
    }

    /// Everyone's hue.
    pub fn hues(&self) -> Memo<HashMap<Uuid, u16>> {
        let by_id = self.by_id;
        Memo::new(move |_| by_id.with(|m| m.iter().map(|(id, (h, _))| (*id, *h)).collect()))
    }
}

/// The people's looks, from the shell (none in a test or a window without them).
pub fn use_people_looks() -> Option<PeopleLooks> {
    use_context::<PeopleLooks>()
}

/// The avatar of the person with this id, with their own colour, shape and name. Shows nothing for
/// someone unknown (an archived person) unless `name` is given, then a grey one.
#[component]
pub fn PersonAvatar(
    id: Uuid,
    #[prop(default = 16)] size: u32,
    #[prop(optional, into)] name: Option<String>,
) -> impl IntoView {
    let looks = use_people_looks();
    move || {
        let look = looks.and_then(|l| l.look(id));
        match (look, name.clone()) {
            (Some((hue, who)), _) => {
                Some(view! { <Avatar name=who hue=Some(hue) size=size /> }.into_any())
            }
            (None, Some(who)) => {
                let unknown: Option<u16> = None;
                Some(view! { <Avatar name=who hue=unknown size=size /> }.into_any())
            }
            (None, None) => None,
        }
    }
}

/// A person's avatar and name side by side: the way a name is shown wherever the person is
/// known by id. Fills the room it is given and shortens the name with "…".
#[component]
pub fn PersonLabel(
    id: Uuid,
    #[prop(into)] name: String,
    #[prop(default = 16)] size: u32,
) -> impl IntoView {
    view! {
        <span class="inline-flex max-w-full min-w-0 items-center gap-1.5 align-middle">
            <span class="shrink-0 leading-none"><PersonAvatar id=id name=name.clone() size=size /></span>
            <span class="min-w-0 truncate">{name}</span>
        </span>
    }
}

/// A node's name, with the person's avatar before it when the node is a person. For lists that
/// mix types (search, links, the palette).
#[component]
pub fn NodeLabel(node: NodeRef, #[prop(into)] name: String) -> impl IntoView {
    if node.node_type == NodeType::Person {
        view! { <PersonLabel id=node.id name=name /> }.into_any()
    } else {
        view! { <span class="min-w-0 truncate">{name}</span> }.into_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_words_use_the_first_and_the_last() {
        assert_eq!(initials("Ada Byron"), "AB");
        assert_eq!(initials("priya nair"), "PN");
        assert_eq!(initials("  Mary Jane Watson "), "MW");
    }

    #[test]
    fn one_word_uses_its_first_two_letters() {
        assert_eq!(initials("Pawan"), "PA");
        assert_eq!(initials("prayrit"), "PR");
        assert_eq!(initials("P"), "P");
    }

    #[test]
    fn an_empty_name_has_no_letters() {
        assert_eq!(initials(""), "");
        assert_eq!(initials("   "), "");
    }

    #[test]
    fn letters_count_characters_not_bytes() {
        assert_eq!(initials("Émile"), "ÉM");
        assert_eq!(initials("ßa"), "SSA");
    }

    #[test]
    fn neighbours_in_the_palette_differ_in_shape() {
        let shapes: Vec<Shape> = PALETTE.iter().map(|h| shape_for(*h)).collect();
        for pair in shapes.windows(2) {
            assert_ne!(pair[0], pair[1]);
        }
        // The first four take all four shapes.
        for (i, a) in shapes[..4].iter().enumerate() {
            for b in &shapes[i + 1..4] {
                assert_ne!(a, b);
            }
        }
    }
}
