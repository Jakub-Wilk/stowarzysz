# frontend-rs: what's left for you

Things only you can do (accounts, secrets, installs) and things to check before relying on the
foundations. Delete items as they're done.

## To do

### 1. Android toolchain (blocks everything Android)

- [ ] Install Android Studio. In its SDK Manager install:
  - SDK Platform 35
  - Android SDK Build-Tools (latest)
  - Android SDK Platform-Tools
  - NDK (Side by side)
  - an emulator system image (x86_64, API 35)
- [ ] Set the environment variables (Windows, user level), then restart the terminal and VS Code:
  - `ANDROID_HOME` = `%LOCALAPPDATA%\Android\Sdk`
  - `ANDROID_NDK_HOME` = `%ANDROID_HOME%\ndk\<version>`
  - `JAVA_HOME` = a JDK 17+ (JDK 21 is already installed)
- [ ] Run `dx doctor` in `frontend-rs/crates/app` and check it reports no Android problems.
- [ ] Ask Claude to continue with the Android build and the self-update test.

### 2. Release key (once, before the first APK goes to anyone)

- [ ] Create the keystore, outside the repo:
  ```sh
  keytool -genkeypair -v -keystore stowarzysz.jks -alias stowarzysz -keyalg RSA -keysize 4096 -validity 36500
  ```
- [ ] Back up the `.jks` and its password together with the `.env.prod` backups. **If either is lost, no installed app can be updated again; everyone has to reinstall by hand.**

### 3. Server (at the v2 cutover)

- [ ] Create a downloads directory on the server (e.g. `/srv/stowarzysz/downloads`) and mount it into the `web` container at `/data/downloads`. It is served as `/downloads/`.
- [ ] Switch the `web` service in `docker-compose.prod.yml` to `frontend-rs/Dockerfile`, built with `--build-arg BUILD=$(git rev-parse --short HEAD)`. This belongs to the cutover, together with the backend-rs switch.
- [ ] Decide the production domain. It goes into `STOWARZYSZ_ORIGIN` when building the APK.

### 4. Firebase (when you want push in the APK)

- [ ] Create a Firebase project and add an Android app with package `com.wilkjakub.stowarzysz`.
- [ ] Download `google-services.json` and keep it next to the keystore. It is gitignored.
- [ ] Create a service account with the "Firebase Cloud Messaging API Admin" role and download its JSON key. It goes to the server as `FCM_SERVICE_ACCOUNT`.
- [ ] Ask Claude to implement phase 6: the FCM sender in backend-rs, the Android push service and token registration.

### 5. Local leftovers from this session

- [ ] Remove the dev admin account `qa` (password `qa-haslo-123`) from the local dev database, or keep it for testing.
- [ ] Stop the Postgres container when you no longer need it: `docker compose stop db`.
- [ ] Review the changes and commit them yourself. Nothing is committed:
  - `frontend-rs/`
  - `backend-rs/crates/server/src/main.rs` (the `openapi` command)
  - `backend-rs/crates/server/tests/openapi_snapshot.rs`
  - `.pre-commit-config.yaml`
  - `AGENTS.md`

## To check

### Web (already verified once; recheck after changes)

- [ ] `cargo test --workspace` in `frontend-rs/` passes.
- [ ] `uv run --project backend pre-commit run --all-files` passes, including the new `frontend-rs` hooks.
- [ ] With backend-rs running (`cargo run` in `backend-rs/`), run `dx serve --platform web` in `frontend-rs/crates/app` and open http://localhost:8080/diagnostyka. Then check:
  - you can sign in
  - the event stream shows "Open", then `ready`
  - creating a poll through http://localhost:8000/api/docs shows `poll.created` in the log
  - signing out clears the session
- [ ] Two tabs signed in at once: both stay signed in after 15+ minutes (token refresh across tabs).
- [ ] Web push: configure VAPID keys in backend-rs (`cargo run -- generate-keys`), then enable notifications in the account panel and receive one. Never tested so far.
- [ ] Build the web image: `docker build --build-arg BUILD=test -t stowarzysz-web frontend-rs`. It has never been built.
- [ ] The deployed site serves `/version.json`. An open tab offers "Odśwież" after a new deploy.

### Android (after step 1)

- [ ] `dx serve --platform android` runs on the emulator. Sign-in through Diagnostyka against `10.0.2.2:8000` works.
- [ ] The status bar takes the page's background colour, and the content isn't hidden under the status or navigation bar.
- [ ] `scripts/build-apk.sh` produces a signed APK. Check `apksigner verify` and that `versionCode` matches the version (0.1.0 → 100).
- [ ] Self-update, with a local nginx serving `/downloads/`:
  1. Install 0.1.0.
  2. Publish 0.1.1 with `scripts/release-apk.sh`.
  3. The app offers the update and asks for "install unknown apps" permission.
  4. It installs, and you are still signed in afterwards.
- [ ] A corrupted APK (change a byte, keep the manifest) is rejected with "Pobrany plik jest uszkodzony".
- [ ] `release-apk.sh --min` blocks older installs with the "Wymagana aktualizacja" screen.
- [ ] Then the same on a real phone, sideloaded from `https://<domain>/downloads/`.

## Decisions still open

- [ ] More themes besides the default (the design). Each is one CSS file redefining the tokens in `crates/app/assets/themes/default.css`.
- [ ] Android minimum version: currently `min_sdk = 26` (Android 8). Raise it if you don't need old phones.
