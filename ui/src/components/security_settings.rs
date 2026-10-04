//! Settings → Encryption (spec 21): turn encryption on with the system keychain or a passphrase,
//! change how the key is kept, show the recovery key once, turn it off, and delete backups that
//! were made before encryption.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{EncryptionChoice, KeyMethod, Secret, SecurityStatus, SetEncryption};

use crate::{
    api,
    components::{
        form::{BUTTON, BUTTON_DANGER, BUTTON_PRIMARY, BUTTON_SOFT, INPUT},
        page::{Card, Tone},
    },
    state::{DataVersion, Toasts},
};

// ------------------------------------------------------------------ pure text

/// One sentence on where things stand.
pub fn status_text(s: &SecurityStatus) -> &'static str {
    match (s.encrypted, s.method) {
        (false, _) => {
            "Encryption is off. Anyone who can read this computer's files can read your data."
        }
        (true, Some(KeyMethod::Keychain)) => {
            "Encrypted. The key is in your system keychain, so Minimap opens without asking."
        }
        (true, Some(KeyMethod::Passphrase)) => {
            "Encrypted. You type your passphrase every time Minimap starts. It is stored nowhere and can't be recovered."
        }
        (true, None) => {
            "Encrypted, but the key isn't stored on this computer: you enter your recovery key at each start."
        }
    }
}

/// What is wrong with a new passphrase and its confirmation, if anything.
pub fn passphrase_problem(pass: &str, confirm: &str, min_chars: u32) -> Option<String> {
    if (pass.chars().count() as u32) < min_chars {
        return Some(format!("Use at least {min_chars} characters"));
    }
    if pass != confirm {
        return Some("The two passphrases are different".to_owned());
    }
    None
}

/// "3 backups are not encrypted".
pub fn plain_backups_text(n: u32) -> String {
    if n == 1 {
        "1 backup is not encrypted: it still holds your data in the clear.".to_owned()
    } else {
        format!("{n} backups are not encrypted: they still hold your data in the clear.")
    }
}

// ------------------------------------------------------------------ the card

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    None,
    /// Turn on, or rotate, with the keychain: explain, then confirm.
    Keychain,
    Passphrase,
    TurnOff,
    DeletePlain,
}

#[component]
pub fn SecuritySettings() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let status = LocalResource::new(move || {
        version.track();
        api::get_security_status()
    });
    // The new recovery key, until the user says it is saved.
    let recovery = RwSignal::new(Option::<String>::None);
    view! {
        <Card title="Encryption"
              description="Encrypts the database file (SQLCipher, AES-256). Backups of an encrypted database are encrypted too.">
            {move || recovery.get().map(|key| view! { <RecoveryKey key=key shown=recovery /> })}
            {move || match status.get() {
                Some(Ok(s)) => view! { <Body status=s recovery=recovery /> }.into_any(),
                Some(Err(e)) => view! { <p class="text-danger">{e.message}</p> }.into_any(),
                None => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Card>
    }
}

#[component]
fn Body(status: SecurityStatus, recovery: RwSignal<Option<String>>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let form = RwSignal::new(Form::None);
    let busy = RwSignal::new(false);
    let pass = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let current = RwSignal::new(String::new());
    let problem = RwSignal::new(Option::<String>::None);
    let min = status.min_passphrase_chars;

    let close = move || {
        form.set(Form::None);
        pass.set(String::new());
        confirm.set(String::new());
        current.set(String::new());
        problem.set(None);
    };
    let apply = move |request: SetEncryption| {
        busy.set(true);
        problem.set(None);
        spawn_local(async move {
            let result = api::set_encryption(request).await;
            busy.set(false);
            match result {
                Ok(done) => {
                    close();
                    if let Some(key) = done.recovery_key {
                        recovery.set(Some(key.0));
                    }
                    toasts.info("Encryption updated");
                    version.bump();
                }
                Err(e) => problem.set(Some(e.message)),
            }
        });
    };
    let use_keychain = move |_| {
        apply(SetEncryption {
            choice: EncryptionChoice::Keychain,
            ..Default::default()
        })
    };
    let use_passphrase = move |_| {
        let (p, c) = (pass.get_untracked(), confirm.get_untracked());
        if let Some(msg) = passphrase_problem(&p, &c, min) {
            problem.set(Some(msg));
            return;
        }
        apply(SetEncryption {
            choice: EncryptionChoice::Passphrase,
            passphrase: Some(Secret(p)),
            ..Default::default()
        });
    };
    let turn_off = move |_| {
        let typed = current.get_untracked();
        apply(SetEncryption {
            choice: EncryptionChoice::Off,
            current_passphrase: (!typed.is_empty()).then_some(Secret(typed)),
            confirmed: true,
            ..Default::default()
        });
    };
    let delete_plain = move |_| {
        busy.set(true);
        spawn_local(async move {
            match api::delete_unencrypted_backups().await {
                Ok(n) => {
                    toasts.info(format!(
                        "Deleted {n} unencrypted backup{}",
                        if n == 1 { "" } else { "s" }
                    ));
                    close();
                    version.bump();
                }
                Err(e) => problem.set(Some(e.message)),
            }
            busy.set(false);
        });
    };

    let encrypted = status.encrypted;
    let method = status.method;
    let keychain_ok = status.keychain_available;
    let keychain_why = status.keychain_problem.clone();
    let plain_backups = status.unencrypted_backups;
    let what = if plain_backups == 1 {
        "the unencrypted backup".to_owned()
    } else {
        format!("the {plain_backups} unencrypted backups")
    };

    let actions = move || {
        let open = move |f: Form| {
            move |_| {
                close();
                form.set(f);
            }
        };
        let keychain_button = |label: &'static str| {
            view! {
                <button class=BUTTON_SOFT disabled=!keychain_ok on:click=open(Form::Keychain)>{label}</button>
            }
        };
        match (encrypted, method) {
            (false, _) => view! {
                {keychain_button("Encrypt with the system keychain (recommended)")}
                <button class=BUTTON on:click=open(Form::Passphrase)>"Encrypt with a passphrase…"</button>
            }.into_any(),
            (true, Some(KeyMethod::Keychain)) => view! {
                <button class=BUTTON on:click=open(Form::Keychain)>"New recovery key"</button>
                <button class=BUTTON on:click=open(Form::Passphrase)>"Use a passphrase instead…"</button>
                <button class=BUTTON_DANGER on:click=open(Form::TurnOff)>"Turn off encryption…"</button>
            }.into_any(),
            (true, Some(KeyMethod::Passphrase)) => view! {
                <button class=BUTTON on:click=open(Form::Passphrase)>"Change passphrase…"</button>
                <button class=BUTTON disabled=!keychain_ok on:click=open(Form::Keychain)>"Use the system keychain"</button>
                <button class=BUTTON_DANGER on:click=open(Form::TurnOff)>"Turn off encryption…"</button>
            }.into_any(),
            (true, None) => view! {
                {keychain_button("Store the key in the system keychain")}
                <button class=BUTTON on:click=open(Form::Passphrase)>"Use a passphrase instead…"</button>
                <button class=BUTTON_DANGER on:click=open(Form::TurnOff)>"Turn off encryption…"</button>
            }.into_any(),
        }
    };

    let panel = move || {
        match form.get() {
        Form::None => ().into_any(),
        Form::Keychain => view! {
            <div class="space-y-2 rounded-sm border border-line bg-canvas p-3">
                <p>"A new random 256-bit key is created and kept in your system keychain. Your data is "
                   "copied into a new encrypted file, checked against the original, and only then does it "
                   "replace it. A backup is taken first."</p>
                <p class="text-muted">"You will be shown the key once, as a recovery key. Keep it somewhere safe: "
                   "it is the only way in if the keychain is ever lost."</p>
                <div class="flex items-center gap-2">
                    <button class=BUTTON_PRIMARY disabled=move || busy.get() on:click=use_keychain>
                        {move || if busy.get() { "Working…" } else { "Continue" }}
                    </button>
                    <button class=BUTTON disabled=move || busy.get() on:click=move |_| close()>"Cancel"</button>
                </div>
            </div>
        }.into_any(),
        Form::Passphrase => view! {
            <div class="space-y-2 rounded-sm border border-line bg-canvas p-3">
                <label class="block space-y-1">
                    <span class="text-[11px] text-muted">{format!("New passphrase (at least {min} characters)")}</span>
                    <input type="password" autocomplete="off" class=INPUT prop:value=move || pass.get()
                           on:input=move |ev| pass.set(event_target_value(&ev)) />
                </label>
                <label class="block space-y-1">
                    <span class="text-[11px] text-muted">"Type it again"</span>
                    <input type="password" autocomplete="off" class=INPUT prop:value=move || confirm.get()
                           on:input=move |ev| confirm.set(event_target_value(&ev)) />
                </label>
                <p class="text-warning">
                    "There is no reset. If you forget this passphrase your data cannot be recovered by anyone, "
                    "including backups made while it was in use."
                </p>
                <div class="flex items-center gap-2">
                    <button class=BUTTON_PRIMARY disabled=move || busy.get() on:click=use_passphrase>
                        {move || if busy.get() { "Working…" } else { "Encrypt with this passphrase" }}
                    </button>
                    <button class=BUTTON disabled=move || busy.get() on:click=move |_| close()>"Cancel"</button>
                </div>
            </div>
        }.into_any(),
        Form::TurnOff => view! {
            <div class="space-y-2 rounded-sm border border-danger/40 bg-danger/5 p-3">
                <p>"Your data file will be readable by anyone with access to this computer's files. "
                   "The data is copied into a new plain file, checked, and replaces the encrypted one."</p>
                {(method == Some(KeyMethod::Passphrase)).then(|| view! {
                    <label class="block space-y-1">
                        <span class="text-[11px] text-muted">"Your current passphrase"</span>
                        <input type="password" autocomplete="off" class=INPUT prop:value=move || current.get()
                               on:input=move |ev| current.set(event_target_value(&ev)) />
                    </label>
                })}
                <div class="flex items-center gap-2">
                    <button class=BUTTON_DANGER disabled=move || busy.get() on:click=turn_off>
                        {move || if busy.get() { "Working…" } else { "Turn off encryption" }}
                    </button>
                    <button class=BUTTON disabled=move || busy.get() on:click=move |_| close()>"Cancel"</button>
                </div>
            </div>
        }.into_any(),
        Form::DeletePlain => view! {
            <div class="space-y-2 rounded-sm border border-danger/40 bg-danger/5 p-3">
                <p>{format!("Delete {what} from the backup folder? Their contents are overwritten and removed. Your encrypted backups are not touched.")}</p>
                <div class="flex items-center gap-2">
                    <button class=BUTTON_DANGER disabled=move || busy.get() on:click=delete_plain>"Delete them"</button>
                    <button class=BUTTON disabled=move || busy.get() on:click=move |_| close()>"Cancel"</button>
                </div>
            </div>
        }.into_any(),
    }
    };

    view! {
        <div class="flex items-center gap-2">
            <span class=if encrypted { Tone::Success.chip() } else { Tone::Warning.chip() }>
                {if encrypted { "encrypted" } else { "not encrypted" }}
            </span>
            <span>{status_text(&status)}</span>
        </div>
        {(!keychain_ok).then(|| view! {
            <p class="text-[11px] text-muted">
                {format!("The system keychain can't be used here, so a passphrase is the way to encrypt. {}",
                         keychain_why.clone().unwrap_or_default())}
            </p>
        })}
        {(encrypted && plain_backups > 0).then(|| view! {
            <div class="flex flex-wrap items-center gap-2 rounded-sm border border-warning/40 bg-warning/5 p-2">
                <span class="text-warning">{plain_backups_text(plain_backups)}</span>
                <button class=BUTTON on:click=move |_| { close(); form.set(Form::DeletePlain); }>"Delete them…"</button>
            </div>
        })}
        <div class="flex flex-wrap items-center gap-2">{actions}</div>
        {panel}
        {move || problem.get().map(|m| view! { <p class="text-danger" role="alert">{m}</p> })}
    }
}

/// The recovery key, once. It stays until the user confirms it is saved.
#[component]
fn RecoveryKey(key: String, shown: RwSignal<Option<String>>) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let saved = RwSignal::new(false);
    let to_copy = key.clone();
    let copy = move |_| {
        let text = to_copy.clone();
        spawn_local(async move {
            match api::copy_text(&text).await {
                Ok(()) => toasts.info("Copied the recovery key"),
                Err(_) => toasts.info("Couldn't copy by itself: select the key and press Ctrl+C"),
            }
        });
    };
    view! {
        <div class="space-y-2 rounded-sm border border-warning/50 bg-warning/5 p-3" role="alertdialog"
             aria-label="Recovery key">
            <p class="font-medium">"Your recovery key"</p>
            <p class="text-muted">
                "This is the only time it is shown. If the system keychain is ever lost or reset, this key (or "
                "your passphrase) is the only way to open your data. Store it in a password manager or print it."
            </p>
            <code class="block select-all break-all rounded-sm border border-line bg-canvas p-2 font-mono text-[13px] leading-6">
                {key}
            </code>
            <div class="flex flex-wrap items-center gap-3">
                <button class=BUTTON on:click=copy>"Copy"</button>
                <label class="flex items-center gap-1.5">
                    <input type="checkbox" prop:checked=move || saved.get()
                           on:change=move |ev| saved.set(event_target_checked(&ev)) />
                    "I have saved this key somewhere safe"
                </label>
                <button class=BUTTON_PRIMARY disabled=move || !saved.get() on:click=move |_| shown.set(None)>"Done"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(encrypted: bool, method: Option<KeyMethod>) -> SecurityStatus {
        SecurityStatus {
            locked: false,
            encrypted,
            method,
            keychain_available: true,
            keychain_problem: None,
            unencrypted_backups: 0,
            min_passphrase_chars: 12,
        }
    }

    #[test]
    fn each_state_has_its_own_sentence() {
        let texts = [
            status_text(&status(false, None)),
            status_text(&status(true, Some(KeyMethod::Keychain))),
            status_text(&status(true, Some(KeyMethod::Passphrase))),
            status_text(&status(true, None)),
        ];
        let unique: std::collections::HashSet<_> = texts.iter().collect();
        assert_eq!(unique.len(), 4);
        assert!(texts[2].contains("can't be recovered"));
        assert!(texts[1].contains("keychain"));
    }

    #[test]
    fn passphrases_are_checked_before_they_are_sent() {
        assert!(passphrase_problem("short", "short", 12)
            .unwrap()
            .contains("12"));
        assert!(
            passphrase_problem("a long enough one", "a long enough two", 12)
                .unwrap()
                .contains("different")
        );
        assert_eq!(
            passphrase_problem("a long enough one", "a long enough one", 12),
            None
        );
        // Characters, not bytes: twelve of these are 36 bytes.
        assert!(passphrase_problem("ééééééééééé", "ééééééééééé", 12).is_some());
        assert_eq!(passphrase_problem("éééééééééééé", "éééééééééééé", 12), None);
    }

    #[test]
    fn unencrypted_backups_are_counted_plainly() {
        assert!(plain_backups_text(1).starts_with("1 backup is"));
        assert!(plain_backups_text(3).starts_with("3 backups are"));
    }
}
