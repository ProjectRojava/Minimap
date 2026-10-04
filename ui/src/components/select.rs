//! A dropdown that follows the theme. The webview's own `<select>` pop-up is drawn by the
//! OS toolkit and ignores page colours, so this is a button plus a list.
//!
//! Keyboard: Enter, Space or the arrow keys open it; arrows move, Enter or Space choose,
//! Esc or Tab close, Home/End jump. The list opens above the button when there is no room below.

use leptos::{ev, html, prelude::*};

use super::{
    form::INPUT,
    overlay::{anchor_of, place, viewport},
};

/// Height of one option row in pixels (`h-6`), used to keep the highlight scrolled into view.
const ROW: f64 = 24.0;
const MAX_LIST: f64 = 256.0;
const MIN_WIDTH: f64 = 140.0;

/// `(value, label)` options. Reports the chosen value; `current` is only the starting value
/// (the control keeps its own selection afterwards, like an uncontrolled `<select>`).
#[component]
pub fn SelectField(
    options: Vec<(String, String)>,
    current: String,
    #[prop(into)] on_change: Callback<String>,
    #[prop(optional)] label: &'static str,
    #[prop(optional)] compact: bool,
    /// DOM id of the button, so keyboard shortcuts can focus the control.
    #[prop(optional, into)]
    id: Option<String>,
) -> impl IntoView {
    let options = StoredValue::new(options);
    let selected = RwSignal::new(current);
    let open = RwSignal::new(false);
    let highlight = RwSignal::new(0usize);
    // Left, top and minimum width of the list, in viewport pixels.
    let pos = RwSignal::new((0.0_f64, 0.0_f64, 0.0_f64));
    let button = NodeRef::<html::Button>::new();
    let list = NodeRef::<html::Ul>::new();

    let count = move || options.with_value(Vec::len);
    // Shows the selected option's label (the first option if the value matches none).
    let shown = move || {
        let v = selected.get();
        options.with_value(|o| {
            o.iter()
                .find(|(val, _)| *val == v)
                .or_else(|| o.first())
                .map(|(_, l)| l.clone())
                .unwrap_or_default()
        })
    };

    let scroll_to = move |i: usize| {
        if let Some(ul) = list.get() {
            let top = i as f64 * ROW;
            let (view_top, view_h) = (f64::from(ul.scroll_top()), f64::from(ul.client_height()));
            if top < view_top {
                ul.set_scroll_top(top as i32);
            } else if top + ROW > view_top + view_h {
                ul.set_scroll_top((top + ROW - view_h) as i32);
            }
        }
    };

    let open_menu = move || {
        let Some(btn) = button.get() else {
            return;
        };
        if count() == 0 {
            return;
        }
        let a = anchor_of(&btn);
        let h = (count() as f64 * ROW + 8.0).min(MAX_LIST);
        let (left, top) = place(&a, a.width.max(MIN_WIDTH), h, viewport());
        pos.set((left, top, a.width));
        let now = selected.get_untracked();
        let at = options
            .with_value(|o| o.iter().position(|(v, _)| *v == now))
            .unwrap_or(0);
        highlight.set(at);
        open.set(true);
        request_animation_frame(move || scroll_to(at));
    };

    let choose = move |i: usize| {
        let Some(value) = options.with_value(|o| o.get(i).map(|(v, _)| v.clone())) else {
            return;
        };
        open.set(false);
        if let Some(b) = button.get() {
            let _ = b.focus();
        }
        if value != selected.get_untracked() {
            selected.set(value.clone());
            on_change.run(value);
        }
    };

    let on_keydown = move |ev: ev::KeyboardEvent| {
        let n = count();
        if n == 0 {
            return;
        }
        let step = |delta: isize| {
            let next =
                (highlight.get_untracked() as isize + delta).clamp(0, n as isize - 1) as usize;
            highlight.set(next);
            scroll_to(next);
        };
        match ev.key().as_str() {
            "ArrowDown" | "ArrowUp" => {
                ev.prevent_default();
                if !open.get_untracked() {
                    open_menu();
                } else if ev.key() == "ArrowDown" {
                    step(1);
                } else {
                    step(-1);
                }
            }
            "Home" if open.get_untracked() => {
                ev.prevent_default();
                highlight.set(0);
                scroll_to(0);
            }
            "End" if open.get_untracked() => {
                ev.prevent_default();
                highlight.set(n - 1);
                scroll_to(n - 1);
            }
            "Enter" | " " => {
                ev.prevent_default();
                if open.get_untracked() {
                    choose(highlight.get_untracked());
                } else {
                    open_menu();
                }
            }
            "Escape" if open.get_untracked() => {
                // Close the list only; don't also close the pane behind it.
                ev.prevent_default();
                ev.stop_propagation();
                open.set(false);
            }
            "Tab" => open.set(false),
            _ => {}
        }
    };

    let button_class = if compact {
        "flex max-w-full items-center justify-between gap-1 rounded-sm border border-line bg-canvas px-1.5 py-0.5 \
         text-left text-[12px] text-fg hover:border-line-strong focus:outline-none focus:border-accent"
    } else {
        // The same look as a text input.
        Box::leak(
            format!("{INPUT} flex items-center justify-between gap-2 text-left").into_boxed_str(),
        )
    };
    let wrapper = if compact {
        "inline-block max-w-full"
    } else {
        "block"
    };

    view! {
        <div class=wrapper>
            {(!label.is_empty()).then(|| view! { <span class="block mb-0.5 text-[11px] text-muted">{label}</span> })}
            <button
                type="button"
                node_ref=button
                id=id
                class=button_class
                aria-haspopup="listbox"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| if open.get_untracked() { open.set(false) } else { open_menu() }
                on:keydown=on_keydown
            >
                <span class="min-w-0 truncate">{shown}</span>
                <span class="shrink-0 text-[9px] text-faint" aria-hidden="true">"▼"</span>
            </button>
            <Show when=move || open.get()>
                // Catches outside clicks (and sits above the button, so clicking it again closes).
                <div class="fixed inset-0 z-[70]" on:mousedown=move |_| open.set(false)></div>
                <ul
                    node_ref=list
                    role="listbox"
                    class="fixed z-[71] overflow-y-auto rounded-sm border border-line bg-panel py-0.5 text-[12px] text-fg"
                    style=move || {
                        let (left, top, width) = pos.get();
                        format!("left:{left}px; top:{top}px; min-width:{}px; max-height:{MAX_LIST}px", width.max(MIN_WIDTH))
                    }
                >
                    {move || options
                        .with_value(|o| o.clone())
                        .into_iter()
                        .enumerate()
                        .map(|(i, (value, text))| view! {
                            <li
                                role="option"
                                aria-selected=move || (selected.get() == value).to_string()
                                class=move || format!(
                                    "flex h-6 cursor-default items-center gap-2 px-2 {}",
                                    if highlight.get() == i { "bg-active" } else { "" })
                                on:mouseenter=move |_| highlight.set(i)
                                // mousedown (not click) so the button keeps focus
                                on:mousedown=move |ev| { ev.prevent_default(); choose(i); }
                            >
                                <span class="w-3 shrink-0 text-[10px]">
                                    {move || if selected.get() == options.with_value(|o| o.get(i).map(|(v, _)| v.clone()).unwrap_or_default()) { "✓" } else { "" }}
                                </span>
                                <span class="truncate">{text}</span>
                            </li>
                        })
                        .collect_view()}
                </ul>
            </Show>
        </div>
    }
}
