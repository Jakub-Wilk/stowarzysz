# stowarzysz

Monorepo with a Django backend and a Vite PWA frontend.

> **Language:** the app is **Polish** (`pl`). The UI, API error messages, push notifications and date formats are all in Polish; the code, comments and developer docs stay in English. See [Language](#language).

| Directory   | Stack                                                                                 |
| ----------- | ------------------------------------------------------------------------------------- |
| `backend/`  | Django 6, Django REST Framework, SimpleJWT, ASGI-only (Daphne), SSE via django-eventstream, PostgreSQL, managed with [uv](https://docs.astral.sh/uv/) |
| `frontend/` | Vite, React, TypeScript, Tailwind CSS v4, shadcn/ui, React Router, TanStack Query, PWA, managed with [pnpm](https://pnpm.io/) |

## Prerequisites

uv, Node 22+, pnpm, Docker (for the local database).

## Setup

```sh
docker compose up -d db                 # Postgres on localhost:5434
cp backend/.env.example backend/.env

(cd backend && uv sync && uv run python manage.py migrate)
(cd frontend && pnpm install)

uv run --project backend pre-commit install   # enable git hooks
```

## Running

```sh
cd backend && uv run python manage.py runserver           # ASGI via Daphne, http://localhost:8000 (Swagger UI at /api/docs/)
cd frontend && pnpm dev                                   # http://localhost:5173, proxies /api to the backend
```

Or use the **Full stack** launch configuration in VS Code ([.vscode/launch.json](.vscode/launch.json)).

## Backend

Plain Django workflow, run from `backend/`:

```sh
uv run python manage.py makemigrations   # + migrate, createsuperuser, shell, ...
uv run pytest                            # needs the Postgres container running
```

- **API:** DRF, JSON only, `IsAuthenticated` by default. OpenAPI schema at `/api/schema/`, Swagger UI at `/api/docs/`.
- **JWT:** `POST /api/auth/token/` (username + password) returns `access` (15 min) and `refresh` (1 year, sliding: rotated and blacklisted on use). Refresh at `/api/auth/token/refresh/`, log out via `/api/auth/token/blacklist/`. Send `Authorization: Bearer <access>`.
- **Users:** custom `accounts.User` model from the start; extend it there.
- **ASGI only:** there is no `wsgi.py`. `daphne` is first in `INSTALLED_APPS`, so `runserver` is an async ASGI server with autoreload.
- **SSE:** `GET /api/events/` streams events for the caller's own `user-<id>` channel. Send with `core.events.notify_user(user_id, "event-type", data)`.
  - Native `EventSource` can't set headers, so use a fetch-based client (e.g. `@microsoft/fetch-event-source`) with the JWT.
  - Listeners live in process memory. To publish from other processes (workers, shell) or run several server processes, set `EVENTSTREAM_REDIS`.
- **Config:** read from environment or `backend/.env` (see `.env.example`).

## Frontend

```sh
pnpm dlx shadcn@latest add dialog   # add shadcn components (Tailwind v4, config in components.json)
pnpm lint | format | typecheck | build
```

## Language

The product is Polish-only; there is no language switcher and no i18n framework.

- **UI text** is written directly in Polish in the components (`frontend/src/`). `index.html` and the PWA manifest declare `lang="pl"`, and dates and counts are formatted for `pl-PL` (use the Polish plural helper in `frontend/src/lib/` for counts).
- **Backend:** `LANGUAGE_CODE = "pl"` and `TIME_ZONE = "Europe/Warsaw"`, so Django and DRF built-in messages (validation, auth, password rules) come out in Polish. Our own API messages and push notification texts are written in Polish in the code.
- **Stays English:** code, identifiers, comments, commit messages, API field names and values, and developer documentation.
- Tests assert on Polish strings where they check messages.

## Code quality

Pre-commit (`.pre-commit-config.yaml`) runs on every commit:

- Backend: `ruff check`, `ruff format`, `ty check` (Django migrations are excluded from ruff)
- Frontend: `oxlint`, `oxfmt`, `tsc`

Run everything manually with `uv run --project backend pre-commit run --all-files`.
