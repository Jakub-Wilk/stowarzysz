# AGENTS.md

Conventions for working in this repo, for humans and coding agents. See [README.md](README.md) for setup.

## Language

**The app is Polish.** Everything a user sees is in Polish: UI text, API error messages (`ValidationError`, `PermissionDenied`, ...), push notification titles and bodies, dates (`pl-PL`) and plurals (Polish has three forms; use the helper in `frontend/src/lib/`, don't write `n === 1 ? ... : ...`). New user-facing strings must be Polish; don't add English ones, and don't introduce an i18n framework unless asked. Code, identifiers, comments, commit messages and these docs stay in English. Use the established terms: *głosowanie* (vote/poll), *weto* (veto), *oddaj głos* (cast vote), *Zarządzanie* (management), *Sejmik* (the voting section), *posłowie* (participants), *wnioskodawca* (poll creator), *obrady* (ongoing votes).

## Layout

```
backend/    Django 6 API (uv, Python 3.14, ASGI-only)
  config/     settings, root urls, asgi.py (no wsgi.py)
  accounts/   custom User model + JWT auth endpoints
  core/       health check, SSE channel manager/helpers, middleware
  tests/      pytest-django tests
  pacts/      bets, resolutions, predictions (kinds in pacts/kinds.py)
  ledger/     minimal money ledger (who owes whom), fed by pacts
frontend/   Vite + React + TypeScript PWA (pnpm)
  src/components/ui/   shadcn components (generated)
.pre-commit-config.yaml, docker-compose.yml (Postgres), .vscode/launch.json (tracked)
```

## Commands

Backend (from `backend/`):

```sh
uv run python manage.py runserver      # Daphne/ASGI dev server on :8000
uv run python manage.py makemigrations && uv run python manage.py migrate
uv run pytest                          # needs `docker compose up -d db`
uv run ruff check --fix . && uv run ruff format . && uv run ty check
```

Frontend (from `frontend/`): `pnpm dev | build | typecheck | lint | format`.

All checks at once: `uv run --project backend pre-commit run --all-files`.

## General

- Keep hooks green; never bypass pre-commit with `--no-verify`. Fix the cause instead.
- Commits should only be authored by humans.
- Add dependencies with `uv add` / `pnpm add` so lockfiles stay in sync, and commit lockfiles.
- Prefer extending the existing patterns below over introducing a second way of doing the same thing.

## Backend

- **Async-first, ASGI only.** Don't add WSGI config or sync-only server assumptions. Use async views or DRF's sync views as appropriate, but never block the event loop in `async def` code (wrap ORM/sync calls with `sync_to_async`, or use Django's async ORM methods).
- **Settings** come from the environment via django-environ (`backend/.env`, template in `.env.example`). Never hard-code secrets, and add new settings with a safe default or make them required.
- **Auth** is JWT only (SimpleJWT). Endpoints are authenticated by default (`IsAuthenticated`); opt out explicitly with `AllowAny` and `authentication_classes([])`, as in `core/views.py`.
- **User model** is `accounts.User`. Always reference it via `settings.AUTH_USER_MODEL` / `get_user_model()`, never `django.contrib.auth.models.User`. People are identified by **username only** (no name or email fields). Profile pictures live in `User.avatar`; set/replace/remove them only via `User.set_avatar` / `clear_avatar` and `accounts.avatars.process_avatar` (validates, square-crops, re-encodes to WebP). Files are served from `MEDIA_URL` (`/api/media/`, so the dev proxy covers it); in production the web server must serve `MEDIA_ROOT` there.
- **Migrations:** one logical change per migration, generated with `makemigrations`, reviewed, and committed with the model change. Never edit an applied migration; add a new one. CI/pre-merge should be clean under `makemigrations --check --dry-run`. Ruff skips `*/migrations`.
- **API shape:** serializers for all request/response bodies so drf-spectacular generates an accurate schema (`/api/schema/`). Keep URLs under `/api/`, with trailing slashes.
- **SSE:** push events with `core.events.notify_user(user_id, event_type, data)`. Channels are per user (`user-<id>`) and `/api/events/` is JWT-authenticated by `core.middleware.EventStreamJWTMiddleware`. Listeners are in process memory, so cross-process publishing needs `EVENTSTREAM_REDIS`.
- **JSON only:** the API accepts and returns JSON (multipart only on file-upload views, via an explicit `parser_classes`). Tests post JSON by default.
- **Voting** (`backend/voting/`): polls are typed by `Poll.kind`; everything kind-specific (config/ballot validation, what a veto means, result and tone) lives in a `PollKind` in `voting/kinds.py`, registered with `register()`. To add a vote type (yes/no, options) add a kind there plus its UI in `frontend/src/features/voting/kinds/`; models, endpoints and the generic screens don't change. All state changes go through `voting/services.py` (locked, validated, events sent on commit). **Ballots are hidden until a poll ends**: that rule lives only in `PollDetailSerializer`, and SSE events carry ids only so clients refetch. Everyone can read every poll; only participants vote and get push. User stats only count closed polls.
  **Profile votes** (`nickname`, `avatar`) are score kinds that change another member's profile: everyone votes, the creator can't target themselves, end them early or be vetoed (`allows_veto`), and they pass at an average of at least 1 with no veto. The change is applied by the kind's `on_close` hook (a failure is recorded in the result as `applied=false`, never raised). A proposed picture is stored on `Poll.proposed_avatar` and deleted when the poll closes. **Deadlines:** every poll ends 72 h after creation (`services.POLL_DURATION`), with push reminders to non-voters at 24/48/69 h. Nothing runs this by itself: schedule `uv run python manage.py process_deadlines` (runs the poll and pact jobs) every ~5 minutes (cron or a systemd timer).
- **Pacts** (`backend/pacts/`, tab "Zakłady"): like polls, typed by `Pact.kind` with everything kind-specific in a `PactKind` in `pacts/kinds.py` (`register()`); all state changes go through `pacts/services.py`. Kinds: `bet` (one wager per opponent against the host, each with its own stake; the loser of a wager owes the winner that stake; wagers accept, settle and fail independently), `group_bet` (shared pot: the winning side splits the losing side's stakes pro rata, nobody loses more than their stake; starts once every invitee has answered), `prediction` (sides, no money) and `resolution` (host commits, the others judge, no money). Invitees must accept (sided kinds take side/stake on accept). Open pacts take join requests (`JoinConsent`): the host approves for everything except `group_bet`, where every active participant must. An outcome claim (`OutcomeProposal`, `{"void": true}` calls it off) settles once every required party confirms; a dispute can be escalated to a `pact_ruling` poll (`voting/kinds.py`) voted on only by members outside the pact, where a strict majority upholds the claim (applied in `on_close`, like profile votes). **Every member can read every pact** (like polls); only participants act on one (403 otherwise), and `is_open` only means anyone may ask to join. Each change is broadcast over SSE (`pact.created` / `pact.updated`, ids only) to everyone; push goes only to the people it concerns. **Money** never lives in pacts: settling calls `ledger.services.record_debt` (integer minor units, `source_type`/`source_id` instead of a foreign key). The ledger is deliberately minimal (debt, mark paid, confirm paid, net balances) and is meant to grow into expense splitting; keep it independent of pacts. Stats (`pacts/stats.py`) come from confirmed claims' `verdicts` plus pact-sourced ledger entries. `process_pact_deadlines` (run via `process_deadlines`) marks overdue pacts, nudges weekly and expires invites after 7 days. `import_pacts` loads the old Google Sheet as a CSV (see its `--help`; `--images` attaches pictures named after the row). **Attachments** (`PactAttachment`) are pictures that supplement a pact's notes: any participant can add up to 10, only via `pacts.services.add_attachment` and `accounts.avatars.process_photo` (validates, keeps the aspect ratio, caps the longest side, re-encodes to WebP, strips EXIF); the uploader or the pact's creator can remove them. Like avatars they are served from `MEDIA_URL`, so anyone with a URL can open the file.
- **Web Push** (`backend/push/`): needs `VAPID_PUBLIC_KEY` / `VAPID_PRIVATE_KEY` / `VAPID_SUBJECT` (`uv run python manage.py generate_vapid_keys`); without them notifications are skipped and the frontend hides the toggle. Delivery runs on a small thread pool after commit. The service worker is `frontend/src/sw.ts` (vite-plugin-pwa `injectManifest`, no precache manifest); it needs HTTPS off localhost. It is push-only: don't add caching or a fetch handler to it, freshness comes from HTTP caching in `frontend/nginx.conf`.
- **ty and Django models:** ty has no Django plugin, so `voting/**` has a few type rules switched off in `pyproject.toml` (model fields are typed as `Field` objects). Prefer narrow `# ty: ignore[rule]` elsewhere.
- **Tests:** pytest-django, tests in `backend/tests/`, shared fixtures in `conftest.py`. Add a test for every endpoint (including the 401 case). Use `APIClient`, not mocks of Django internals.
- **Typing:** annotate function signatures. `ty` is beta; if it reports a false positive, use a narrow `# ty: ignore[rule]` with the specific rule, not a blanket ignore.

## Frontend

- TypeScript `strict`. No `any`; type API responses explicitly (ideally generated from `/api/schema/`).
- Use the `@/` import alias for `src/` instead of long relative paths.
- **UI:** Tailwind v4 (CSS-first, config lives in `src/index.css`, no `tailwind.config.js`). Add components with `pnpm dlx shadcn@latest add <name>`; they land in `src/components/ui/` and are excluded from oxlint. Prefer composing them over editing them. Use `cn()` from `@/lib/utils` for conditional classes.
- **Data fetching:** TanStack Query for all server state, with caching off on purpose (`staleTime: 0`, `gcTime: 0` in `src/main.tsx`): always-fresh data matters more than saved requests. Don't add `staleTime`/`gcTime` to queries. No fetching in `useEffect`. Routing is React Router (`src/App.tsx`).
- **Auth/SSE:** send `Authorization: Bearer <access token>` on API calls. Native `EventSource` can't set headers, so use a fetch-based SSE client for `/api/events/`.
- **Themes vs. color:** a theme (`src/themes/<id>.css`) is styling only; its palette is derived from one base hue, `--base-hue` (oklch hue 0-360), e.g. `oklch(0.26 0.15 var(--base-hue))` or `calc(var(--base-hue) + 45)` for related hues. The user's hue lives in `src/themes/color.ts` (localStorage `base-hue`, set inline on `<html>`; unset falls back to the theme's default in CSS). There is no UI yet: use `setBaseColor(150)`, `setBaseColor('#2a9d8f')` or `setBaseColor()` in the console. Vote tones and a theme's identity colors (royal's gold, christmas) stay fixed.
- **PWA:** configured in `vite.config.ts` (vite-plugin-pwa). Icons in `public/` are generated from the root `logo.png` (`pwa-*`, `maskable-512x512.png` padded on the theme color, `apple-touch-icon.png`, `favicon.png`). Dev proxies `/api` to `localhost:8000`, so use relative `/api/...` URLs.
- **Motion:** use the shared system in `src/components/motion/` (read its `README.md`): tokens and `animate-*` utilities in `src/index.css`, `Stagger` / `Reveal` / `PageTransition` / `Skeleton` / `useCountUp` / `useFlashOnChange`. Animate transform/opacity only, take colors from theme tokens, and don't add a second animation approach. Reduced motion is handled globally in `index.css`.
- Format with oxfmt and lint with oxlint (config in `frontend/.oxfmtrc.json` and `.oxlintrc.json`). Don't add ESLint or Prettier.

## Deploying

`scripts/deploy.sh` (see [README.md](README.md#deploying)) is the only supported way to deploy: `master` is production, no tags. It keeps at most 3 DB snapshots in `backups/`, rolls back code (never the DB) on a failed launch and installs the `process_deadlines` cron job. Keep it shellcheck-clean and idempotent, and don't add a second deploy path.

## Gotchas

- Local Postgres is on host port **5434** (5432/5433 are commonly taken).
- If oxlint or oxfmt fail with "Cannot find native binding", run `pnpm install --force` in `frontend/`.
- If the ty editor extension reports unresolved imports after changing dependencies, restart its server.
