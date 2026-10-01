# AGENTS.md

Conventions for working in this repo, for humans and coding agents. See [README.md](README.md) for setup.

## Layout

```
backend/    Django 6 API (uv, Python 3.14, ASGI-only)
  config/     settings, root urls, asgi.py (no wsgi.py)
  accounts/   custom User model + JWT auth endpoints
  core/       health check, SSE channel manager/helpers, middleware
  tests/      pytest-django tests
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
- **Web Push** (`backend/push/`): needs `VAPID_PUBLIC_KEY` / `VAPID_PRIVATE_KEY` / `VAPID_SUBJECT` (`uv run python manage.py generate_vapid_keys`); without them notifications are skipped and the frontend hides the toggle. Delivery runs on a small thread pool after commit. The service worker is `frontend/src/sw.ts` (vite-plugin-pwa `injectManifest`); it needs HTTPS off localhost.
- **ty and Django models:** ty has no Django plugin, so `voting/**` has a few type rules switched off in `pyproject.toml` (model fields are typed as `Field` objects). Prefer narrow `# ty: ignore[rule]` elsewhere.
- **Tests:** pytest-django, tests in `backend/tests/`, shared fixtures in `conftest.py`. Add a test for every endpoint (including the 401 case). Use `APIClient`, not mocks of Django internals.
- **Typing:** annotate function signatures. `ty` is beta; if it reports a false positive, use a narrow `# ty: ignore[rule]` with the specific rule, not a blanket ignore.

## Frontend

- TypeScript `strict`. No `any`; type API responses explicitly (ideally generated from `/api/schema/`).
- Use the `@/` import alias for `src/` instead of long relative paths.
- **UI:** Tailwind v4 (CSS-first, config lives in `src/index.css`, no `tailwind.config.js`). Add components with `pnpm dlx shadcn@latest add <name>`; they land in `src/components/ui/` and are excluded from oxlint. Prefer composing them over editing them. Use `cn()` from `@/lib/utils` for conditional classes.
- **Data fetching:** TanStack Query for all server state. No fetching in `useEffect`. Routing is React Router (`src/App.tsx`).
- **Auth/SSE:** send `Authorization: Bearer <access token>` on API calls. Native `EventSource` can't set headers, so use a fetch-based SSE client for `/api/events/`.
- **PWA:** configured in `vite.config.ts` (vite-plugin-pwa). The icons in `public/` are placeholders. Dev proxies `/api` to `localhost:8000`, so use relative `/api/...` URLs.
- Format with oxfmt and lint with oxlint (config in `frontend/.oxfmtrc.json` and `.oxlintrc.json`). Don't add ESLint or Prettier.

## Gotchas

- Local Postgres is on host port **5434** (5432/5433 are commonly taken).
- If oxlint or oxfmt fail with "Cannot find native binding", run `pnpm install --force` in `frontend/`.
- If the ty editor extension reports unresolved imports after changing dependencies, restart its server.
