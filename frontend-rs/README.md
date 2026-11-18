# stowarzysz frontend (Dioxus)

Frontend v2: one Rust codebase shipped as a **PWA** (same origin as the API, behind nginx) and as
a sideloaded **Android APK that updates itself**. It talks to `../backend-rs/` and replaces
`../frontend/` at the v2 cutover. The UI is Polish, like everything else (see `../AGENTS.md`).

```
crates/api/        the API: types generated from openapi.json, session/refresh client, SSE parser
  openapi.json     committed snapshot of backend-rs's contract (scripts/update-openapi.sh)
crates/app/        the Dioxus app (features `web`, `mobile`)
  Dioxus.toml      app id com.wilkjakub.stowarzysz, dev proxy, Android settings
  tailwind.css     Tailwind v4 input (tokens, themes, motion); dx compiles it to assets/tailwind.css
  index.html       PWA head (manifest, theme-color, pre-paint of the remembered theme)
  public/          web-only files served at the root: manifest, sw.js (push only), icons
  assets/          themes/<id>.css (the tokens), ui/*.css (component styles, tokens only),
                   decor/*.svg (fixed-colour decorations), fonts/ (self-hosted), logo
  android/         MainActivity.kt: the activity plus the Kotlin side of JNI (installer, system bars)
  src/
    platform/      everything platform-specific: web.rs, android.rs, host.rs (tests only)
    session.rs     the API client and who is signed in
    query.rs       use_query + the refetch bus (no cache, like the React app)
    realtime.rs    /api/events -> refetch bus; session.revoked -> sign out
    updates.rs     version.json (web) / android.json (APK) checks, banner, blocking screen
    theme/         theme registry, event override, <html data-theme>
    components/    shared parts: Desk, AppHeader (+ profile bookmark), Module/ModuleSurface,
                   PaperLabel, Stamp, Avatar, foil; Button and icons for plain screens
    layout/        guards and `Screen` (desk, header, page, profile menu)
    home/          the home screen: module cards, birthday banner, the counters (summary.rs)
    profile/       the profile menu (legitymacja); only WYLOGUJ works so far
    pages/         login (plain form for now), placeholders for module pages, /diagnostyka
    polish/        plurals, money (grosze), dates (Warsaw time, Polish names)
scripts/           update-openapi.sh, build-apk.sh, release-apk.sh, fetch-fonts.py, fetch-icons.py
Dockerfile, nginx.conf   the web image
```

## Setup

```sh
cargo binstall dioxus-cli@0.7.10   # dx; keep it equal to the dioxus version in crates/app/Cargo.toml
# rust-toolchain.toml installs the wasm and Android targets on first use.
```

For Android also: Android Studio (or the command-line tools) with an SDK platform (35), build-tools,
platform-tools, the NDK and an emulator image; set `ANDROID_HOME` and `ANDROID_NDK_HOME`
(and `JAVA_HOME` to a JDK 17+). `dx doctor` checks the setup.

## Commands (from `crates/app/` unless noted)

```sh
dx serve --platform web            # http://localhost:8080, /api proxied to backend-rs on :8000
dx serve --platform android        # emulator/device; reaches the host's backend at 10.0.2.2:8000
cargo test --workspace             # from frontend-rs/: api client + app logic (host build)
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p stowarzysz --features web --target wasm32-unknown-unknown -- -D warnings
scripts/update-openapi.sh          # after a backend-rs API change (backend-rs tests fail until you do)
```

Sign in at `/login` (an unstyled form until the designed one) to reach the home screen.
`/diagnostyka` (unlinked) shows the build, session, live events, a test query, push and updates.

## How it fits together

- **Platform code lives in `src/platform/` only.** Each target implements the same functions
  (storage, origin, sleep, token store, system bars, push, updater); nothing else uses `cfg`.
  Browser APIs without a sync answer go through `document::eval`, which also works in the Android
  WebView. JNI (`jni` + `ndk-context`) is confined to `platform/android.rs`, the one module allowed
  `unsafe`.
- **Auth:** `api::Client` keeps the access token in memory and the refresh token in the platform's
  `TokenStore` (`localStorage["auth.refresh"]`, the React app's key, so PWA users stay signed in;
  a private file on Android). Refreshes are single-flight and, on the web, serialized across tabs
  with `navigator.locks` (the backend rotates refresh tokens, so two tabs must never refresh at
  once). A rejected refresh signs out; offline or 5xx keeps the session.
- **Server state:** `use_query(&[Topic::...], fetch)` refetches on mount, when an SSE event bumps
  one of its topics, on `resync` or a reconnect's `ready` (not the first one, which arrives right
  after the queries started), and when the app returns to the foreground. There is no
  cache on purpose (always-fresh data), as in the React app. Events map to topics in
  `realtime::topics`, mirroring `frontend/src/features/voting/realtime.tsx`.
- **Look and theming:** the design is the "Stowarzysz – nowe głosowanie" canvas (page "Final
  design"): a desk with paper, folders, stamps and wax seals. `assets/themes/default.css` holds it
  as tokens (desk, papers, inks, stamp red, bookmark and foil, one colour pair per module, the
  four fonts); `assets/ui/*.css` style the components from those tokens only, so a new theme
  (one CSS file scoped to `[data-theme='<id>']` plus an entry in `src/theme/mod.rs`) restyles
  everything. Decorations with fixed colours (Santa hat, bunting, confetti) are SVG files in
  `assets/decor/`; the garland is generated for its card's size (`home/garland.rs`). Fonts are self-hosted, latin + latin-ext (`scripts/fetch-fonts.py`: Old
  Standard TT, Special Elite, Caveat). Module screens are built from the same parts as the home
  cards: `ModuleSurface` (the coloured binder) and `PaperLabel` (title, subtitle, stamps).
- **Home counters** come from the module endpoints, one query per card (`home/summary.rs`, pure
  counting functions with tests): polls to answer, payments to confirm and the balance, overdue
  pacts, Secret Santa idea requests; the Mikołajki card shows only while an event runs.
- **Staying current:** the web image writes `version.json` (the git commit, `STOWARZYSZ_BUILD`); a
  tab running another build offers to reload. The APK reads `/downloads/android.json`, downloads
  the APK, checks its size and SHA-256, and installs it through a `PackageInstaller` session;
  below `min_version_code` the app shows only "Zaktualizuj".

## Android releases

One-time:

1. Create the release key and keep it (with its password) next to the deployment's `.env.prod`
   backups. **If it is lost, no installed app can be updated again; everyone reinstalls by hand.**
   ```sh
   keytool -genkeypair -v -keystore stowarzysz.jks -alias stowarzysz -keyalg RSA -keysize 4096 -validity 36500
   ```
2. On the server, give nginx a downloads directory: mount it into the `web` container at
   `/data/downloads` (served as `/downloads/`).

Every release:

```sh
# bump `version` in crates/app/Cargo.toml (minor and patch stay below 100: versionCode = M*10000+m*100+p)
STOWARZYSZ_ORIGIN=https://<domain> ANDROID_KEYSTORE=/path/stowarzysz.jks ANDROID_KEYSTORE_PASSWORD=... \
  scripts/build-apk.sh
RELEASE_TARGET=user@host:/srv/stowarzysz/downloads scripts/release-apk.sh [--min] [--notes "..."]
```

`--min` forces every older install to update before it can be used (for API changes old clients
can't follow). The first install is a sideload of `https://<domain>/downloads/stowarzysz-<v>.apk`;
Android asks once to allow installing from the browser, and then from the app itself for updates.

## Status

- Verified: the web build end to end against backend-rs (sign-in, refresh, SSE, refetch, theme
  switching, tabs, sign-out), all unit tests, the nginx config, the release script.
- **Not yet built:** the Android side (`platform/android.rs`, `android/MainActivity.kt`,
  `scripts/build-apk.sh`) is written but untested until the Android SDK/NDK are installed.
  Known unknowns: whether `ndk_context` is initialised by Dioxus's Android glue (it should be, via
  wry), edge-to-edge insets in the WebView, and the exact `dx bundle` output path.
- Push on Android (FCM) is phase 6 of the plan: backend sender, `PushService`, token
  registration. Until then the toggle is hidden in the APK.
