//! `/diagnostyka`: the QA harness for the foundations. Not linked from the app (only from the
//! placeholder login screen); it shows the build, the session, the live event stream, push and
//! updates, and has a bare login form until the real one exists.

use std::cell::Cell;
use std::rc::Rc;

use api::types::Me;
use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Icon, Spinner, Svg};
use crate::platform::{self, PushStatus};
use crate::query::{Topic, use_query};
use crate::realtime::use_realtime;
use crate::routes::Route;
use crate::session::{Auth, use_client, use_session};
use crate::theme::use_theme;
use crate::updates::{Phase, use_updates};
use crate::{config, polish::dates};

#[component]
fn Section(title: String, children: Element) -> Element {
    rsx! {
        section { class: "flex flex-col gap-2 rounded-xl border bg-card p-4 text-card-foreground",
            h2 { class: "font-semibold", "{title}" }
            {children}
        }
    }
}

#[component]
fn Row(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex justify-between gap-4 text-sm",
            span { class: "text-muted-foreground", "{label}" }
            span { class: "text-right break-all", "{value}" }
        }
    }
}

#[component]
pub fn Diagnostics() -> Element {
    let session = use_session();
    let auth = session.auth.read().clone();
    let theme = use_theme();
    rsx! {
        div { class: "mx-auto flex min-h-svh max-w-2xl flex-col gap-4 p-4 pt-[calc(1rem+env(safe-area-inset-top))]",
            div { class: "flex items-center justify-between",
                h1 { class: "text-2xl font-semibold", "Diagnostyka" }
                Link { to: Route::Home {}, class: "text-sm underline underline-offset-4", "Do aplikacji" }
            }
            Section { title: "Wersja",
                Row { label: "Wersja", value: format!("{} (kod {})", config::VERSION, config::version_code()) }
                Row { label: "Kompilacja", value: config::BUILD }
                Row { label: "Platforma", value: format!("{:?}", platform::KIND) }
                Row { label: "Serwer", value: platform::api_origin() }
                Row { label: "Motyw", value: format!("{} ({})", theme.effective().label(), theme.effective().id()) }
                Row { label: "Teraz (Warszawa)", value: dates::now().format("%Y-%m-%d %H:%M:%S").to_string() }
            }
            Section { title: "Sesja",
                match auth {
                    Auth::Checking => rsx! { Spinner {} },
                    Auth::SignedOut => rsx! { LoginForm {} },
                    Auth::Unreachable(message) => rsx! {
                        p { class: "text-sm text-destructive", "{message}" }
                        Button { small: true, onclick: move |_| session.reload(), "Spróbuj ponownie" }
                    },
                    Auth::SignedIn(me) => rsx! { SignedInInfo { me } },
                }
            }
            if matches!(*session.auth.read(), Auth::SignedIn(_)) {
                EventsSection {}
                QuerySection {}
                div { class: "rounded-xl border bg-card p-4 text-card-foreground", PushToggle { verbose: true } }
            }
            UpdatesSection {}
        }
    }
}

#[component]
fn LoginForm() -> Element {
    let session = use_session();
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    rsx! {
        form {
            class: "flex flex-col gap-2",
            onsubmit: move |event| async move {
                event.prevent_default();
                busy.set(true);
                error.set(None);
                if let Err(e) = session.login(&username(), &password()).await {
                    error.set(Some(e.message().to_owned()));
                }
                busy.set(false);
            },
            input {
                class: "h-10 rounded-md border bg-background px-3",
                placeholder: "Nazwa użytkownika",
                autocomplete: "username",
                value: "{username}",
                oninput: move |e| username.set(e.value()),
            }
            input {
                class: "h-10 rounded-md border bg-background px-3",
                r#type: "password",
                placeholder: "Hasło",
                autocomplete: "current-password",
                value: "{password}",
                oninput: move |e| password.set(e.value()),
            }
            if let Some(message) = error() {
                p { class: "animate-wiggle text-sm text-destructive", "{message}" }
            }
            Button { submit: true, disabled: busy(), "Zaloguj się" }
        }
    }
}

#[component]
fn SignedInInfo(me: ReadSignal<Me>) -> Element {
    let session = use_session();
    let me = me();
    let role = if me.is_superuser {
        "administrator"
    } else {
        "poseł"
    };
    rsx! {
        Row { label: "Zalogowano jako", value: format!("{} (#{}, {role})", me.username, me.id) }
        div { class: "flex gap-2",
            Button { variant: ButtonVariant::Outline, small: true, onclick: move |_| session.reload(), "Odśwież /api/me" }
            Button {
                variant: ButtonVariant::Outline,
                small: true,
                onclick: move |_| async move { session.logout().await },
                "Wyloguj się"
            }
        }
    }
}

#[component]
fn EventsSection() -> Element {
    let realtime = use_realtime();
    let status = *realtime.status.read();
    rsx! {
        Section { title: "Strumień zdarzeń",
            Row { label: "Stan", value: format!("{status:?}") }
            ul { class: "max-h-64 overflow-y-auto font-mono text-xs",
                for (i, line) in realtime.log.read().iter().enumerate() {
                    li { key: "{i}", class: "border-b py-1 break-all", "{line}" }
                }
            }
        }
    }
}

/// A query on the `Me` topic: it refetches on `ready`/`resync`, `poll.closed` and on focus.
#[component]
fn QuerySection() -> Element {
    let client = use_client();
    let fetches = use_hook(|| Rc::new(Cell::new(0u32)));
    let counter = fetches.clone();
    let me = use_query(&[Topic::Me], move || {
        let client = client.clone();
        counter.set(counter.get() + 1);
        async move { client.get::<Me>("/api/me").await }
    });
    let state = match &*me.read() {
        None => "ładowanie…".to_owned(),
        Some(Ok(me)) => format!("ok ({})", me.username),
        Some(Err(error)) => format!("błąd: {}", error.message()),
    };
    rsx! {
        Section { title: "Zapytanie testowe (/api/me)",
            Row { label: "Pobrano", value: format!("{} razy", fetches.get()) }
            Row { label: "Wynik", value: state }
        }
    }
}

#[component]
fn UpdatesSection() -> Element {
    let updates = use_updates();
    let client = use_client();
    let phase = updates.phase.read().clone();
    let manifest = updates.manifest.read().clone();
    let state = match &phase {
        Phase::Idle => "nie sprawdzono".to_owned(),
        Phase::Checking => "sprawdzanie…".to_owned(),
        Phase::UpToDate => "aktualna".to_owned(),
        Phase::Reload => "nowa wersja strony: odśwież".to_owned(),
        Phase::Available => "dostępna aktualizacja".to_owned(),
        Phase::Downloading { done, total } => format!("pobieranie {done}/{total} B"),
        Phase::Installing => "instalowanie".to_owned(),
        Phase::NeedsPermission => "brak zgody na instalowanie".to_owned(),
        Phase::Failed(error) => format!("błąd: {error}"),
    };
    rsx! {
        Section { title: "Aktualizacje",
            Row { label: "Stan", value: state }
            if let Some(manifest) = manifest {
                Row {
                    label: "Najnowsza",
                    value: format!("{} (kod {}, min. {})", manifest.version_name, manifest.version_code, manifest.min_version_code),
                }
            }
            Button {
                variant: ButtonVariant::Outline,
                small: true,
                class: "self-start",
                onclick: move |_| {
                    let mut dismissed = updates.dismissed;
                    dismissed.set(false);
                    spawn(updates.check(client.clone(), true));
                },
                "Sprawdź aktualizacje"
            }
        }
    }
}

/// The notifications toggle (hidden when the server has no push keys, unless `verbose`).
#[component]
pub fn PushToggle(#[props(default)] verbose: bool) -> Element {
    let client = use_client();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut status = {
        let client = client.clone();
        use_resource(move || {
            let client = client.clone();
            async move { platform::push::status(&client).await }
        })
    };
    let current = *status.read();
    let toggle = move |enable: bool| {
        let client = client.clone();
        async move {
            busy.set(true);
            error.set(None);
            let result = if enable {
                platform::push::enable(&client).await
            } else {
                platform::push::disable(&client).await
            };
            if let Err(message) = result {
                error.set(Some(message));
            }
            busy.set(false);
            status.restart();
        }
    };
    let body = match current {
        None if verbose => rsx! { Spinner { size: 16 } },
        None => return rsx! {},
        Some(PushStatus::Unconfigured) if verbose => rsx! {
            p { class: "text-xs text-muted-foreground", "Serwer nie ma skonfigurowanych powiadomień." }
        },
        Some(PushStatus::Unconfigured) => return rsx! {},
        Some(PushStatus::Unsupported) => rsx! {
            p { class: "text-xs text-muted-foreground", "To urządzenie nie obsługuje powiadomień." }
        },
        Some(PushStatus::Blocked) => rsx! {
            p { class: "text-xs text-muted-foreground",
                "Powiadomienia są zablokowane. Włącz je w ustawieniach przeglądarki lub telefonu."
            }
        },
        Some(PushStatus::Off) => {
            let toggle = toggle.clone();
            rsx! {
                Button {
                    variant: ButtonVariant::Outline,
                    small: true,
                    disabled: busy(),
                    onclick: move |_| toggle(true),
                    Svg { icon: Icon::Bell, size: 16 }
                    "Włącz powiadomienia"
                }
            }
        }
        Some(PushStatus::On) => {
            let toggle = toggle.clone();
            rsx! {
                Button {
                    variant: ButtonVariant::Outline,
                    small: true,
                    disabled: busy(),
                    onclick: move |_| toggle(false),
                    Svg { icon: Icon::BellOff, size: 16 }
                    "Wyłącz powiadomienia"
                }
            }
        }
    };
    rsx! {
        section { class: "flex flex-col gap-2",
            h2 { class: "text-sm font-semibold", "Powiadomienia" }
            {body}
            if let Some(message) = error() {
                p { class: "text-xs text-destructive", "{message}" }
            }
        }
    }
}
