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
