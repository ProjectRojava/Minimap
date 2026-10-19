//! The webview's content security policy (`tauri.conf.json`), checked as text, because a mistake
//! only shows in an installed build: `cargo tauri dev` serves the UI from the dev server, which
//! the policy is not applied to, so a policy that blocks the app's own files looks fine until the
//! AppImage or the .deb opens to an empty window.

fn directives() -> Vec<(String, Vec<String>)> {
    let conf: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json is JSON");
    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("the config sets a csp")
        .to_owned();
    csp.split(';')
        .filter_map(|d| {
            let mut words = d.split_whitespace().map(str::to_owned);
            let name = words.next()?;
            Some((name, words.collect()))
        })
        .collect()
}

fn sources(name: &str) -> Vec<String> {
    directives()
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, s)| s)
        .unwrap_or_default()
}

#[test]
fn the_app_may_load_its_own_script_and_wasm() {
    // The page is a module script plus a `.wasm` file fetched at start-up.
    let script = sources("script-src");
    assert!(script.contains(&"'self'".to_owned()), "{script:?}");
    assert!(
        script.contains(&"'wasm-unsafe-eval'".to_owned()),
        "{script:?}"
    );
    // `fetch` (which loads the `.wasm`) is governed by connect-src, which does not fall back to
    // default-src once it is set: it needs 'self' itself, next to the IPC schemes.
    let connect = sources("connect-src");
    assert!(
        connect.contains(&"'self'".to_owned()),
        "connect-src: {connect:?}"
    );
    assert!(
        connect.contains(&"ipc:".to_owned()),
        "connect-src: {connect:?}"
    );
}

#[test]
fn nothing_loads_from_the_internet() {
    for (name, list) in directives() {
        for source in list {
            assert!(
                !source.starts_with("https://") && source != "*" && source != "'unsafe-eval'",
                "{name} allows {source}"
            );
        }
    }
}

#[test]
fn inline_style_attributes_stay_allowed_in_an_installed_build() {
    // Tauri adds a nonce to `style-src` for the inline <style> in index.html when it embeds the
    // page, and a browser then ignores 'unsafe-inline' there, so every `style="..."` the UI sets
    // (positions, colours, grid templates) is dropped in an installed build while `cargo tauri
    // dev` (no policy) looks fine. `style-src-attr` is not touched by Tauri and says it again.
    let attr = sources("style-src-attr");
    assert!(
        attr.contains(&"'unsafe-inline'".to_owned()),
        "style-src-attr: {attr:?}"
    );
}
