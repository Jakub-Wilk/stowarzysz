//! `/login`: a plain form for now (the designed login screen comes later). Signed-in visitors go
//! straight to the home screen.

use dioxus::prelude::*;

use crate::routes::Route;
use crate::session::{Auth, use_session};

#[component]
pub fn Login() -> Element {
    let session = use_session();
    let navigator = use_navigator();
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    use_effect(move || {
        if matches!(*session.auth.read(), Auth::SignedIn(_)) {
            navigator.replace(Route::Home {});
        }
    });
    rsx! {
        main { class: "login-plain",
            h1 { "Logowanie" }
            form {
                onsubmit: move |event| async move {
                    event.prevent_default();
                    busy.set(true);
                    error.set(None);
                    if let Err(e) = session.login(&username(), &password()).await {
                        error.set(Some(e.message().to_owned()));
                    }
                    busy.set(false);
                },
                label {
                    "Nazwa użytkownika"
                    input {
                        name: "username",
                        autocomplete: "username",
                        required: true,
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                    }
                }
                label {
                    "Hasło"
                    input {
                        name: "password",
                        r#type: "password",
                        autocomplete: "current-password",
                        required: true,
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                    }
                }
                if let Some(message) = error() {
                    p { role: "alert", "{message}" }
                }
                button { r#type: "submit", disabled: busy(), "Zaloguj się" }
            }
        }
    }
}
