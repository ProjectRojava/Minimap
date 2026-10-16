//! The radar window (docs/product-design/10-radar-window.md, spec 53, ADR-0021): a small frameless
//! window that shows your world centred on you. It is its own page, mounted instead of the whole
//! app when the window is named `radar`: no sidebar, title bar, status bar or detail pane.
//!
//! Glance, hover for what a mark is, click to open it in the main window. Nothing here edits.
//! What goes where is decided in `minimap_core::radar`; `radar_geom` turns that into drawing
//! coordinates; this file draws it and polls for changes.

use std::time::Duration;

use leptos::{ev, html, prelude::*, task::spawn_local, web_sys};
use minimap_types::{
    AppError, Blip, BlipKind, BlipSize, EdgeMark, FogLevel, NodeRef as Item, Radar, RadarMark,
    Severity, View,
};

use crate::{
    api,
    components::{
        objective_colour::hue_for,
        orientation::{load_view, save_view, view_from_storage},
        page::Icon,
    },
    radar_geom::{
        arc_path, blip_radius, chevron_path, from_view, hit_test, point, rim_point, ring_label,
        ring_radius, scale_of, spoke_path, step_focus, to_view, tooltip_origin, tooltip_size,
        Target, CENTRE, HIT_TOLERANCE_PX, RING_RADII,
    },
    theme::ThemeCtx,
    window::{self, Platform, ResizeDir},
};

/// How often the radar asks whether anything changed, and how many of those asks make a
/// minute (it refreshes anyway then: dates move at midnight and meetings come round).
const POLL: Duration = Duration::from_secs(5);
const POLLS_PER_MINUTE: u32 = 12;

// ------------------------------------------------------------------ pure wording

/// What the radar says when it cannot read Minimap (the main window may be locked or closed).
pub fn error_text(e: &AppError) -> String {
    match e.code.as_str() {
        "locked" => "Minimap is locked. Unlock it in the main window.".to_owned(),
        _ => format!("Can't read Minimap: {}", e.message),
    }
}

/// The hover text of the view button.
pub fn view_title(shown: View, chosen: View) -> String {
    match (shown, chosen) {
        (View::Portfolio, View::Mine) => {
            "My view needs you set in People first. Click to go back to Portfolio view".to_owned()
        }
        (View::Portfolio, _) => "Portfolio view: everything. Click for My view (v)".to_owned(),
        (View::Mine, _) => "My view: what touches you. Click for Portfolio view (v)".to_owned(),
    }
}

/// The pin button's hover text.
pub fn pin_title(on_top: bool) -> &'static str {
    if on_top {
        "Staying on top of other windows. Click to let it go behind them"
    } else {
        "Behind other windows when you use them. Click to keep it on top"
    }
}

/// The opacity of a blip by how long nobody touched what it stands for.
fn fog_opacity(fog: FogLevel) -> &'static str {
    match fog {
        FogLevel::Clear => "1",
        FogLevel::Haze => "0.75",
        FogLevel::Fog => "0.5",
    }
}

fn mark_class(mark: RadarMark, kind: BlipKind) -> &'static str {
    match (kind, mark) {
        (BlipKind::Ping, _) => "text-accent",
        (BlipKind::Meeting, _) => "text-muted",
        (_, RadarMark::Red) => "text-danger",
        (_, RadarMark::Amber) => "text-warning",
        (_, RadarMark::Green) => "text-success",
        (_, RadarMark::None) => "text-muted",
    }
}

/// Where the targets of the pointer are: every blip, then every edge mark.
fn targets_of(radar: &Radar) -> Vec<Target> {
    let blips = radar.blips.iter().map(|b| {
        let (x, y) = point(b.r, b.theta);
        Target {
            x,
            y,
            radius: blip_radius(b.size) + 1.0,
        }
    });
    let marks = radar.edge_marks.iter().map(|m| {
        let (x, y) = rim_point(m.theta, 3.5);
        Target { x, y, radius: 5.0 }
    });
    blips.chain(marks).collect()
}

/// Which thing an index from [`targets_of`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Blip(usize),
    Mark(usize),
}

fn pick_of(radar: &Radar, index: usize) -> Pick {
    if index < radar.blips.len() {
        Pick::Blip(index)
    } else {
        Pick::Mark(index - radar.blips.len())
    }
}

fn label_of(radar: &Radar, pick: Pick) -> Option<String> {
    match pick {
        Pick::Blip(i) => radar.blips.get(i).map(|b| b.label.clone()),
        Pick::Mark(i) => radar.edge_marks.get(i).map(|m| m.label.clone()),
    }
}

/// What opening a thing opens: a blip is its item, an edge mark is the thing in trouble.
fn node_of(radar: &Radar, pick: Pick) -> Option<Item> {
    match pick {
        Pick::Blip(i) => radar.blips.get(i).map(|b| b.node.node),
        Pick::Mark(i) => radar.edge_marks.get(i).map(|m| m.source.node),
    }
}

fn place_of(radar: &Radar, pick: Pick) -> Option<(f64, f64)> {
    match pick {
        Pick::Blip(i) => radar.blips.get(i).map(|b| point(b.r, b.theta)),
        Pick::Mark(i) => radar.edge_marks.get(i).map(|m| rim_point(m.theta, 3.5)),
    }
}

// ------------------------------------------------------------------ the drawing

/// One blip: the shape carries the meaning (health is filled, half or empty; a meeting is a
/// square; a ping is a pin), colour only repeats it.
fn blip_shape(b: &Blip) -> impl IntoView {
    let r = blip_radius(b.size);
    let class = mark_class(b.mark, b.kind);
    let body = match (b.kind, b.mark) {
        (BlipKind::Meeting, _) => view! {
            <rect x=-r y=-r width=2.0 * r height=2.0 * r fill="none"
                  stroke="currentColor" stroke-width="1.2" />
        }
        .into_any(),
        (BlipKind::Ping, _) => view! {
            <path d="M0 4.6C-3 1.2-3.7-1.4-3.7-2.4a3.7 3.7 0 0 1 7.4 0C3.7-1.4 3 1.2 0 4.6z"
                  fill="none" stroke="currentColor" stroke-width="1.2" />
            <circle cx="0" cy="-2.4" r="1.2" fill="currentColor" />
        }
        .into_any(),
        (_, RadarMark::Red) => view! {
            <circle r=r fill="currentColor" stroke="currentColor" stroke-width="1" />
        }
        .into_any(),
        (_, RadarMark::Amber) => view! {
            <circle r=r fill="none" stroke="currentColor" stroke-width="1.2" />
            <path d=format!("M0 {}A{r} {r} 0 0 0 0 {r}z", -r) fill="currentColor" />
        }
        .into_any(),
        (_, RadarMark::Green) => view! {
            <circle r=r fill="none" stroke="currentColor" stroke-width="1.2" />
        }
        .into_any(),
        (_, RadarMark::None) => view! {
            <path d=format!("M{} 0H{r}", -r) stroke="currentColor" stroke-width="1.6"
                  stroke-linecap="round" />
        }
        .into_any(),
    };
    // A project has a thin ring round it, so it reads as bigger than a task of the same size.
    let project_ring = (b.kind == BlipKind::Project).then(|| {
        view! { <circle r=r + 2.0 fill="none" stroke="currentColor" stroke-width="0.6" /> }
    });
    // Fog: the same blip, fainter, inside a dashed outline.
    let fog_ring = (b.fog == FogLevel::Fog).then(|| {
        view! {
            <circle r=r + 2.8 fill="none" stroke="currentColor" stroke-width="0.7"
                    stroke-dasharray="1.2 1.2" />
        }
    });
    // A pin badge when the item has a ping.
    let pin = (b.pinged && b.kind != BlipKind::Ping).then(|| {
        view! {
            <circle cx=r * 0.9 cy=-r * 0.9 r="1.7" class="fill-accent" />
        }
    });
    let (x, y) = point(b.r, b.theta);
    view! {
        <g transform=format!("translate({x:.2} {y:.2})") class=class
           style=format!("opacity:{}", fog_opacity(b.fog))>
            {body}
            {project_ring}
            {fog_ring}
            {pin}
        </g>
    }
}

/// The edge mark: a short arc on the rim and a chevron out from it, in `warning` when the work
/// it threatens has little room and `danger` when it is already late. It pulses twice when it
/// first appears (CSS; none under reduced motion).
fn edge_mark_shape(m: &EdgeMark) -> impl IntoView {
    let class = match m.severity {
        Severity::Hits => "radar-pulse text-danger",
        _ => "radar-pulse text-warning",
    };
    let arc = arc_path(1.0, 3.0, m.theta - 0.06, 0.12);
    let chevron = chevron_path(m.theta);
    view! {
        <g class=class>
            <path d=arc fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
            <path d=chevron fill="none" stroke="currentColor" stroke-width="1.4"
                  stroke-linecap="round" stroke-linejoin="round" />
        </g>
    }
}

/// The rings, sector dividers and rims, ring labels and you in the middle.
fn backdrop(radar: &Radar) -> impl IntoView {
    let rings = RING_RADII
        .iter()
        .map(|r| view! { <circle cx=CENTRE cy=CENTRE r=ring_radius(*r) fill="none" class="stroke-line" stroke-width="1" /> })
        .collect_view();
    let labels = RING_RADII
        .iter()
        .zip(radar.rings)
        .map(|(r, days)| {
            view! {
                <text x=CENTRE + 1.5 y=CENTRE - ring_radius(*r) - 1.2 font-size="4.5"
                      class="fill-faint">{ring_label(days)}</text>
            }
        })
        .collect_view();
    let many = radar.sectors.len() > 1;
    let spokes = radar
        .sectors
        .iter()
        .filter(|_| many)
        .map(|s| view! { <path d=spoke_path(s.start) fill="none" class="stroke-line" stroke-width="1" /> })
        .collect_view();
    let rims = radar
        .sectors
        .iter()
        .filter_map(|s| {
            let rank = s.hue_rank?;
            Some(view! {
                <path d=arc_path(1.0, 0.0, s.start, s.sweep) class="radar-rim" stroke-width="3"
                      style=format!("--obj-h: {}", hue_for(rank as usize)) />
            })
        })
        .collect_view();
    view! {
        <circle cx=CENTRE cy=CENTRE r=ring_radius(1.0) fill="none" class="stroke-line" stroke-width="1" />
        {rings}
        {spokes}
        {rims}
        {labels}
        // You: a small triangle, pointing up.
        <path d="M100 94.5L104.2 103.5L100 101.4L95.8 103.5z" class="fill-accent" />
    }
}

fn rect_of(area: NodeRef<html::Div>) -> Option<(f64, f64, f64, f64)> {
    let r = area.get_untracked()?.get_bounding_client_rect();
    Some((r.left(), r.top(), r.width(), r.height()))
}

// ------------------------------------------------------------------ the screen

/// The radar page: applies the theme and shows [`RadarScreen`].
#[component]
pub fn RadarApp() -> impl IntoView {
    let theme = ThemeCtx::new();
    provide_context(theme);
    // The database is the source of truth for the theme; localStorage only speeds up start-up.
    let settings = LocalResource::new(api::get_settings);
    Effect::new(move |_| {
        if let Some(Ok(s)) = settings.get() {
            theme.select(&s.theme);
        }
    });
    view! { <RadarScreen settings=settings /> }
}

#[component]
fn RadarScreen(
    settings: LocalResource<Result<minimap_types::Settings, AppError>>,
) -> impl IntoView {
    let chosen = RwSignal::new(load_view());
    let radar = RwSignal::new(None::<Radar>);
    let error = RwSignal::new(None::<AppError>);
    let on_top = RwSignal::new(true);
    let hover = RwSignal::new(None::<Pick>);
    let focus = RwSignal::new(None::<usize>);
    let pointer = RwSignal::new((0.0_f64, 0.0_f64));
    let area = NodeRef::<html::Div>::new();
    // What the last answer was stamped with, and how many polls since the last full refresh.
    let revision = StoredValue::new(i64::MIN);
    let polls = StoredValue::new(0_u32);

    Effect::new(move |_| {
        if let Some(Ok(s)) = settings.get() {
            on_top.set(s.radar_on_top);
        }
    });

    let refresh = move || {
        let view = chosen.get_untracked();
        spawn_local(async move {
            match api::get_radar(view).await {
                Ok(r) => {
                    revision.set_value(r.revision);
                    polls.set_value(0);
                    // The keyboard focus follows a blip that is still there, else lets go.
                    if focus.get_untracked().is_some_and(|i| i >= r.blips.len()) {
                        focus.set(None);
                    }
                    hover.set(None);
                    radar.set(Some(r));
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    // At start, and whenever the view changes (here or in the main window).
    Effect::new(move |_| {
        chosen.track();
        refresh();
    });
    // Ask the cheap question every few seconds; ask for the whole picture only when the answer
    // moved, or once a minute anyway.
    let poll = move || {
        spawn_local(async move {
            let minute = polls.try_update_value(|n| {
                *n += 1;
                *n >= POLLS_PER_MINUTE
            });
            match api::get_data_revision().await {
                Ok(now) if now != revision.get_value() || minute == Some(true) => refresh(),
                Ok(_) => {}
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let handle = set_interval_with_handle(poll, POLL).ok();
    on_cleanup(move || {
        if let Some(h) = handle {
            h.clear();
        }
    });
    // A toggle in the main window arrives as a `storage` event.
    let follow = window_event_listener(ev::storage, move |e: web_sys::StorageEvent| {
        if let Some(view) = view_from_storage(e.key().as_deref(), e.new_value().as_deref()) {
            chosen.set(view);
        }
    });
    on_cleanup(move || follow.remove());

    let toggle_view = move || {
        let next = chosen.get_untracked().toggled();
        save_view(next);
        chosen.set(next);
    };
    let open_pick = move |pick: Pick| {
        let node = radar.with_untracked(|r| r.as_ref().and_then(|r| node_of(r, pick)));
        if let Some(node) = node {
            spawn_local(async move {
                if let Err(e) = api::open_in_main(node).await {
                    error.set(Some(e));
                }
            });
        }
    };
    let close = move || {
        spawn_local(async move {
            let _ = api::close_radar().await;
        });
    };
    let toggle_pin = move || {
        let next = !on_top.get_untracked();
        on_top.set(next);
        spawn_local(async move {
            if let Err(e) = api::set_radar_on_top(next).await {
                on_top.set(!next);
                error.set(Some(e));
            }
        });
    };

    // Keyboard: Tab walks the blips in angle order, Enter opens, Esc closes, v switches view.
    let keys = window_event_listener(ev::keydown, move |e: web_sys::KeyboardEvent| {
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let count = radar.with_untracked(|r| r.as_ref().map_or(0, |r| r.blips.len()));
        match e.key().as_str() {
            "Tab" => {
                e.prevent_default();
                focus.set(step_focus(focus.get_untracked(), count, !e.shift_key()));
                hover.set(None);
            }
            "Enter" => {
                let pick = hover
                    .get_untracked()
                    .or(focus.get_untracked().map(Pick::Blip));
                if let Some(pick) = pick {
                    e.prevent_default();
                    open_pick(pick);
                }
            }
            "Escape" => close(),
            "v" => toggle_view(),
            _ => {}
        }
    });
    on_cleanup(move || keys.remove());

    // Pointer: hover for the words, click to open, drag the empty space to move the window.
    let on_move = move |e: ev::MouseEvent| {
        let Some((left, top, w, h)) = rect_of(area) else {
            return;
        };
        let (px, py) = (
            f64::from(e.client_x()) - left,
            f64::from(e.client_y()) - top,
        );
        pointer.set((px, py));
        let (x, y) = to_view(px, py, w, h);
        let found = radar.with_untracked(|r| {
            r.as_ref().and_then(|r| {
                hit_test(&targets_of(r), x, y, HIT_TOLERANCE_PX, scale_of(w, h))
                    .map(|i| pick_of(r, i))
            })
        });
        if found != hover.get_untracked() {
            hover.set(found);
        }
    };
    let on_down = move |e: ev::MouseEvent| {
        if e.button() != 0 || hover.get_untracked().is_some() {
            return;
        }
        spawn_local(async move {
            let _ = window::start_dragging().await;
        });
    };
    let on_click = move |_| {
        if let Some(pick) = hover.get_untracked() {
            open_pick(pick);
        }
    };

    // The words by the pointer (or by the blip the keyboard is on).
    let tooltip = move || {
        let (pick, at_pointer) = match (hover.get(), focus.get()) {
            (Some(p), _) => (p, true),
            (None, Some(i)) => (Pick::Blip(i), false),
            (None, None) => return None,
        };
        let r = radar.get()?;
        let text = label_of(&r, pick)?;
        let (_, _, w, h) = rect_of(area)?;
        let at = if at_pointer {
            pointer.get()
        } else {
            let (x, y) = place_of(&r, pick)?;
            from_view(x, y, w, h)
        };
        let size = tooltip_size(&text, (w - 8.0).min(230.0));
        let (x, y) = tooltip_origin(at, size, (w, h));
        Some(view! {
            <div class="pointer-events-none absolute z-10 rounded-sm border border-line-strong bg-panel \
                        px-1.5 py-1 text-[11px] leading-[15px] text-fg"
                 style=format!("left:{x:.0}px;top:{y:.0}px;width:{:.0}px", size.0)
                 role="tooltip">{text}</div>
        })
    };

    let marks = move || radar.get().map(|r| r.edge_marks).unwrap_or_default();
    let footer = move || match error.get() {
        Some(e) => error_text(&e),
        None => radar.with(|r| {
            r.as_ref()
                .map_or_else(|| "Reading Minimap…".to_owned(), |r| r.footer.text())
        }),
    };
    let shown_view = move || radar.with(|r| r.as_ref().map_or(chosen.get(), |r| r.view));
    let platform = Platform::detect();

    view! {
        <div class="relative flex h-screen w-screen select-none flex-col overflow-hidden bg-canvas text-fg">
            <header class="flex h-5 shrink-0 items-center gap-0.5 border-b border-line bg-panel px-1"
                    on:mousedown=move |e| {
                        if e.button() == 0 {
                            spawn_local(async move { let _ = window::start_dragging().await; });
                        }
                    }>
                <button class="inline-flex h-4 w-5 items-center justify-center rounded-sm text-muted hover:bg-hover hover:text-fg"
                        aria-label="Switch between Portfolio view and My view"
                        title=move || view_title(shown_view(), chosen.get())
                        on:mousedown=|e| e.stop_propagation()
                        on:click=move |_| toggle_view()>
                    {move || view! {
                        <Icon name=if shown_view() == View::Mine { "me" } else { "north" } size="h-3 w-3" />
                    }}
                </button>
                <button class="inline-flex h-4 w-5 items-center justify-center rounded-sm hover:bg-hover"
                        class=("text-accent", move || on_top.get())
                        class=("text-muted", move || !on_top.get())
                        aria-label="Keep on top of other windows"
                        aria-pressed=move || on_top.get().to_string()
                        title=move || pin_title(on_top.get())
                        on:mousedown=|e| e.stop_propagation()
                        on:click=move |_| toggle_pin()>
                    <Icon name="pin-top" size="h-3 w-3" />
                </button>
                <span class="flex-1"></span>
                <button class="inline-flex h-4 w-5 items-center justify-center rounded-sm text-muted hover:bg-danger hover:text-canvas"
                        aria-label="Close the radar" title="Close the radar (Esc)"
                        on:mousedown=|e| e.stop_propagation()
                        on:click=move |_| close()>
                    <svg width="8" height="8" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.2">
                        <path d="M1 1l8 8M9 1L1 9" />
                    </svg>
                </button>
            </header>
            <div node_ref=area class="relative min-h-0 flex-1"
                 class=("cursor-pointer", move || hover.get().is_some())
                 on:mousemove=on_move
                 on:mouseleave=move |_| hover.set(None)
                 on:mousedown=on_down
                 on:click=on_click>
                <svg class="absolute inset-0 h-full w-full" viewBox="0 0 200 200"
                     role="img" aria-label="Radar: your work by when it is due and which objective it serves">
                    {move || radar.get().map(|r| backdrop(&r))}
                    <For each=marks
                         key=|m| (m.label.clone(), (m.theta * 1000.0).round() as i64)
                         children=|m| edge_mark_shape(&m) />
                    {move || radar.get().map(|r| {
                        r.blips.iter().map(blip_shape).collect_view()
                    })}
                    // The blip under the pointer or the keyboard gets a ring.
                    {move || {
                        let r = radar.get()?;
                        let pick = hover.get().or(focus.get().map(Pick::Blip))?;
                        let (x, y) = place_of(&r, pick)?;
                        let size = match pick {
                            Pick::Blip(i) => r.blips.get(i).map_or(BlipSize::M, |b| b.size),
                            Pick::Mark(_) => BlipSize::M,
                        };
                        Some(view! {
                            <circle cx=x cy=y r=blip_radius(size) + 4.0 fill="none"
                                    class="stroke-accent" stroke-width="1" />
                        })
                    }}
                </svg>
                {tooltip}
            </div>
            <footer class="flex h-6 shrink-0 items-center border-t border-line bg-panel px-2 text-[11px] text-muted"
                    class=("text-danger", move || error.get().is_some())>
                <span class="truncate" title=footer>{footer}</span>
            </footer>
            {(platform != Platform::Windows).then(|| view! { <EdgeGrips /> })}
        </div>
    }
}

/// Grab areas round the window's edge: an undecorated window has no resize border of its own
/// on Linux or macOS (Windows gives one natively).
#[component]
fn EdgeGrips() -> impl IntoView {
    view! {
        {ResizeDir::ALL.iter().map(|&dir| view! {
            <div class=format!("fixed z-[60] {}", dir.class())
                 on:mousedown=move |e| {
                     e.prevent_default();
                     e.stop_propagation();
                     spawn_local(async move { let _ = window::start_resize(dir).await; });
                 }></div>
        }).collect_view()}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(code: &str, message: &str) -> AppError {
        AppError {
            code: code.into(),
            message: message.into(),
        }
    }

    #[test]
    fn a_locked_database_is_explained_and_other_errors_are_quoted() {
        assert_eq!(
            error_text(&err("locked", "x")),
            "Minimap is locked. Unlock it in the main window."
        );
        assert_eq!(
            error_text(&err("ipc", "no bridge")),
            "Can't read Minimap: no bridge"
        );
    }

    #[test]
    fn the_view_button_says_which_view_and_what_a_click_does() {
        assert!(view_title(View::Portfolio, View::Portfolio).contains("Click for My view"));
        assert!(view_title(View::Mine, View::Mine).contains("Click for Portfolio view"));
        assert!(view_title(View::Portfolio, View::Mine).contains("People"));
    }

    #[test]
    fn the_pin_says_what_it_will_do() {
        assert!(pin_title(true).contains("let it go behind"));
        assert!(pin_title(false).contains("keep it on top"));
    }

    #[test]
    fn fog_fades_the_blip_and_health_has_its_own_colour_but_never_only_colour() {
        assert_eq!(fog_opacity(FogLevel::Clear), "1");
        assert!(fog_opacity(FogLevel::Fog) < fog_opacity(FogLevel::Haze));
        assert_eq!(mark_class(RadarMark::Red, BlipKind::Task), "text-danger");
        assert_eq!(
            mark_class(RadarMark::Amber, BlipKind::Project),
            "text-warning"
        );
        assert_eq!(mark_class(RadarMark::Green, BlipKind::Task), "text-success");
        assert_eq!(mark_class(RadarMark::None, BlipKind::Meeting), "text-muted");
        assert_eq!(mark_class(RadarMark::None, BlipKind::Ping), "text-accent");
    }
}
