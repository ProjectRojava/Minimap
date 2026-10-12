//! "Are you sure?" for every archive (spec 42): one dialog, mounted once, asked by every place
//! that archives something. It says what will happen, offers the way back (Ctrl/Cmd+Z), and
//! starts with Cancel focused so a stray Enter archives nothing.

use leptos::{ev, html, prelude::*};

use crate::components::form::{BUTTON, BUTTON_DANGER};

/// One way to go ahead: its button text and what it does. The dialog closes after it runs.
#[derive(Clone)]
pub struct ArchiveChoice {
    pub label: String,
    pub run: Callback<()>,
}

/// What the dialog shows.
#[derive(Clone)]
pub struct ArchiveRequest {
    /// The question: `Archive "Moody's Orbis"?`.
    pub title: String,
    /// What will happen to it and what hangs off it.
    pub message: String,
    /// Anything more the person should see or choose from (the tasks of a project).
    pub body: Option<ViewFn>,
    /// The ways to go ahead, the first being the main one. Cancel is always there.
    pub choices: Vec<ArchiveChoice>,
}

/// The request being asked, if any. Provided once by the app.
#[derive(Clone, Copy)]
pub struct ConfirmArchive(
    pub RwSignal<Option<ArchiveRequest>>,
    StoredValue<Option<Owner>>,
);

/// What every archive says about getting it back.
pub const UNDO_HINT: &str = "Ctrl/Cmd+Z brings it back.";

impl ConfirmArchive {
    pub fn new() -> Self {
        // Remember the app's owner: a confirm callback made while a menu or row is handling a
        // click would otherwise belong to that menu or row, and be dropped as soon as it closes,
        // before the person has pressed *Archive*.
        Self(RwSignal::new(None), StoredValue::new(Owner::current()))
    }

    /// The common case: one *Archive* button.
    pub fn ask(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        on_confirm: impl Fn() + Send + Sync + 'static,
    ) {
        let make = move || Callback::new(move |()| on_confirm());
        let run = match self.1.get_value() {
            Some(owner) => owner.with(make),
            None => make(),
        };
        self.ask_with(ArchiveRequest {
            title: title.into(),
            message: message.into(),
            body: None,
            choices: vec![ArchiveChoice {
                label: "Archive".to_owned(),
                run,
            }],
        });
    }

    pub fn ask_with(&self, request: ArchiveRequest) {
        self.0.set(Some(request));
    }

    pub fn close(&self) {
        self.0.set(None);
    }
}

impl Default for ConfirmArchive {
    fn default() -> Self {
        Self::new()
    }
}

/// `Archive "X"?` for a named item.
pub fn question(name: &str) -> String {
    format!("Archive \"{name}\"?")
}

/// Mount once, inside the router.
#[component]
pub fn ConfirmArchiveHost() -> impl IntoView {
    let confirm = expect_context::<ConfirmArchive>();
    view! {
        {move || confirm.0.get().map(|request| view! { <Dialog request=request /> })}
    }
}

#[component]
fn Dialog(request: ArchiveRequest) -> impl IntoView {
    let confirm = expect_context::<ConfirmArchive>();
    let cancel = NodeRef::<html::Button>::new();
    Effect::new(move |_| {
        if let Some(el) = cancel.get() {
            let _ = el.focus();
        }
    });
    let on_keydown = move |e: ev::KeyboardEvent| {
        // Nothing typed here reaches the shortcuts underneath.
        e.stop_propagation();
        if e.key() == "Escape" {
            e.prevent_default();
            confirm.close();
        }
    };
    let buttons = request
        .choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let run = choice.run;
            let class = if i == 0 { BUTTON_DANGER } else { BUTTON };
            view! {
                <button type="button" class=class
                    on:click=move |_| {
                        confirm.close();
                        run.run(());
                    }>{choice.label.clone()}</button>
            }
        })
        .collect_view();
    view! {
        <div class="fixed inset-0 z-[90] bg-scrim" on:mousedown=move |_| confirm.close()></div>
        <div role="alertdialog" aria-label=request.title.clone()
             class="fixed inset-x-0 mx-auto top-[18vh] z-[91] w-[28rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-2 text-[14px] font-semibold">{request.title.clone()}</h2>
            <p class="mb-3 text-muted">{request.message.clone()}</p>
            {request.body.as_ref().map(|b| b.run())}
            <div class="mt-3 flex flex-wrap justify-end gap-2">
                {buttons}
                <button type="button" class=BUTTON node_ref=cancel on:click=move |_| confirm.close()>
                    "Cancel"
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_names_the_item() {
        assert_eq!(question("Moody's"), "Archive \"Moody's\"?");
    }

    #[test]
    fn asking_sets_the_request_and_closing_clears_it() {
        let owner = Owner::new();
        owner.with(|| {
            let c = ConfirmArchive::new();
            c.ask("Archive it?", "Gone from the lists.", || {});
            let r = c.0.get_untracked().expect("a request");
            assert_eq!(r.choices.len(), 1);
            assert_eq!(r.choices[0].label, "Archive");
            c.close();
            assert!(c.0.get_untracked().is_none());
        });
    }
}
