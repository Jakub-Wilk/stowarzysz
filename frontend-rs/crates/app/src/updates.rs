//! Staying current. API changes ship together with the frontend, so an old client must notice:
//! - web: `/version.json` names the deployed build; a tab running another offers to reload;
//! - Android: `/downloads/android.json` (published by `scripts/release-apk.sh`) describes the
//!   newest APK; the app downloads it, checks its SHA-256 and hands it to the system installer.
//!   Below `min_version_code` the app refuses to run until updated.

use chrono::{DateTime, TimeDelta, Utc};
use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{Button, ButtonVariant, Icon, Svg};
use crate::config;
use crate::platform::{self, Kind, updater};
use crate::session::use_client;

/// `/downloads/android.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub version_code: u32,
    pub version_name: String,
    /// The APK, relative to the API origin.
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// Older installs must update before they can be used.
    #[serde(default)]
    pub min_version_code: u32,
    #[serde(default)]
    pub notes: String,
}

/// `/version.json` (written by the web image's build).
#[derive(Debug, Deserialize)]
struct WebVersion {
    build: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// A newer web build is deployed: reload.
    Reload,
    /// A newer APK (in `Updates::manifest`).
    Available,
    Downloading {
        done: u64,
        total: u64,
    },
    /// Handed to the system installer.
    Installing,
    /// The user must allow installing from this app first.
    NeedsPermission,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Updates {
    pub phase: Signal<Phase>,
    pub manifest: Signal<Option<Manifest>>,
    pub dismissed: Signal<bool>,
    last_check: Signal<Option<DateTime<Utc>>>,
}

/// How often coming back to the app checks again.
fn interval() -> TimeDelta {
    match platform::KIND {
        Kind::Android => TimeDelta::hours(6),
        _ => TimeDelta::minutes(10),
    }
}

impl Updates {
    /// Whether this install is too old to be used.
    pub fn required(&self) -> bool {
        self.manifest
            .read()
            .as_ref()
            .is_some_and(|m| config::version_code() < m.min_version_code)
    }

    /// Check now (`force`) or if the last check is older than the interval.
    pub async fn check(mut self, client: api::Client, force: bool) {
        let now = Utc::now();
        let due = self
            .last_check
            .peek()
            .is_none_or(|last| now - last >= interval());
        let busy = matches!(
            *self.phase.peek(),
            Phase::Checking | Phase::Downloading { .. } | Phase::Installing
        );
        if busy || !(force || due) {
            return;
        }
        self.last_check.set(Some(now));
        self.phase.set(Phase::Checking);
        let phase = match platform::KIND {
            Kind::Web if config::BUILD == "dev" => Phase::UpToDate,
            Kind::Web => {
                let path = format!("/version.json?t={}", now.timestamp());
                match client.get_public::<WebVersion>(&path).await {
                    Ok(deployed) if deployed.build != config::BUILD => Phase::Reload,
                    Ok(_) => Phase::UpToDate,
                    Err(error) => Phase::Failed(error.message().to_owned()),
                }
            }
            Kind::Android => {
                match client
                    .get_public::<Manifest>("/downloads/android.json")
                    .await
                {
                    Ok(manifest) => {
                        let newer = manifest.version_code > config::version_code();
                        self.manifest.set(Some(manifest));
                        if newer {
                            Phase::Available
                        } else {
                            Phase::UpToDate
                        }
                    }
                    Err(error) => Phase::Failed(error.message().to_owned()),
                }
            }
            Kind::Host => Phase::UpToDate,
        };
        self.phase.set(phase);
    }

    /// Download, verify and install the APK in `manifest` (Android).
    pub async fn install(mut self, client: api::Client) {
        let Some(manifest) = self.manifest.peek().clone() else {
            return;
        };
        if !updater::can_install() {
            self.phase.set(Phase::NeedsPermission);
            return;
        }
        self.phase.set(Phase::Downloading {
            done: 0,
            total: manifest.size,
        });
        let mut phase = self.phase;
        let result = updater::download_and_install(&client, &manifest, |done, total| {
            phase.set(Phase::Downloading { done, total });
        })
        .await;
        self.phase.set(match result {
            Ok(()) => Phase::Installing,
            Err(error) => Phase::Failed(error),
        });
    }
}

pub fn use_updates_provider() -> Updates {
    let updates = use_context_provider(|| Updates {
        phase: Signal::new(Phase::Idle),
        manifest: Signal::new(None),
        dismissed: Signal::new(false),
        last_check: Signal::new(None),
    });
    let client = use_client();
    use_hook(move || {
        // A failed install reported by the system installer since the last launch.
        if let Some(error) = updater::take_error() {
            let mut phase = updates.phase;
            phase.set(Phase::Failed(error));
        }
        spawn(updates.check(client, true));
    });
    updates
}

pub fn use_updates() -> Updates {
    use_context()
}

fn megabytes(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mb = bytes as f64 / 1_048_576.0;
    format!("{mb:.1}").replace('.', ",")
}

/// What to do about an update, as a banner (or, when required, a screen of its own).
#[component]
pub fn UpdateBanner() -> Element {
    let updates = use_updates();
    let client = use_client();
    let phase = updates.phase.read().clone();
    let manifest = updates.manifest.read().clone();
    let required = updates.required();
    let mut dismissed = updates.dismissed;

    let (message, action): (String, Option<(&str, EventHandler<MouseEvent>)>) = match phase {
        Phase::Reload => (
            "Dostępna jest nowa wersja aplikacji.".to_owned(),
            Some((
                "Odśwież",
                EventHandler::new(|_| {
                    document::eval("location.reload()");
                }),
            )),
        ),
        Phase::Available => {
            let name = manifest
                .as_ref()
                .map(|m| m.version_name.clone())
                .unwrap_or_default();
            let client = client.clone();
            (
                format!("Dostępna aktualizacja {name}."),
                Some((
                    "Zaktualizuj",
                    EventHandler::new(move |_| {
                        spawn(updates.install(client.clone()));
                    }),
                )),
            )
        }
        Phase::Downloading { done, total } => (
            format!(
                "Pobieranie aktualizacji: {} z {} MB…",
                megabytes(done),
                megabytes(total)
            ),
            None,
        ),
        Phase::Installing => ("Potwierdź instalację w oknie systemu.".to_owned(), None),
        Phase::NeedsPermission => (
            "Aby zainstalować aktualizację, zezwól tej aplikacji na instalowanie aplikacji, \
             a potem wróć i spróbuj ponownie."
                .to_owned(),
            Some((
                "Otwórz ustawienia",
                EventHandler::new(|_| updater::open_install_settings()),
            )),
        ),
        Phase::Failed(error) if updater::SUPPORTED && manifest.is_some() => {
            let client = client.clone();
            (
                format!("Aktualizacja się nie udała: {error}"),
                Some((
                    "Spróbuj ponownie",
                    EventHandler::new(move |_| {
                        spawn(updates.install(client.clone()));
                    }),
                )),
            )
        }
        Phase::Idle | Phase::Checking | Phase::UpToDate | Phase::Failed(_) => return rsx! {},
    };

    if required {
        return rsx! {
            div { class: "fixed inset-0 z-50 flex flex-col items-center justify-center gap-6 bg-background p-8 text-center",
                Svg { icon: Icon::Download, size: 48 }
                h1 { class: "text-2xl font-semibold", "Wymagana aktualizacja" }
                p { class: "max-w-sm text-muted-foreground",
                    "Ta wersja aplikacji jest już nieobsługiwana. {message}"
                }
                if let Some((label, onclick)) = action {
                    Button { onclick: move |e| onclick.call(e), "{label}" }
                }
            }
        };
    }
    if *dismissed.read() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "animate-fade-up mx-3 mt-2 flex items-center gap-3 rounded-lg border bg-card px-3 py-2 text-sm text-card-foreground shadow",
            role: "status",
            Svg { icon: Icon::Download, size: 18 }
            p { class: "flex-1", "{message}" }
            if let Some((label, onclick)) = action {
                Button { variant: ButtonVariant::Primary, small: true, onclick: move |e| onclick.call(e), "{label}" }
            }
            Button {
                variant: ButtonVariant::Icon,
                small: true,
                aria_label: "Zamknij",
                onclick: move |_| dismissed.set(true),
                Svg { icon: Icon::X, size: 16 }
            }
        }
    }
}
