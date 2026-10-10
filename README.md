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

### Database shell (development)

With the Postgres container running (`docker compose up -d db`), open a `psql` shell inside it:

```sh
docker compose exec db psql -U postgres stowarzysz
```

Or, from `backend/`, let Django pick up `DATABASE_URL` for you (needs `psql` installed locally):

```sh
uv run python manage.py dbshell
```

Or connect from the host with any client: `postgres://postgres:postgres@localhost:5434/stowarzysz`. Handy `psql` commands: `\dt` (list tables), `\d voting_poll` (describe a table), `\q` (quit).

For production, see [Operations](#operations).

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

## Deployment (Docker)

[docker-compose.prod.yml](docker-compose.prod.yml) runs three containers:

| Service   | What it does                                                                                       |
| --------- | -------------------------------------------------------------------------------------------------- |
| `db`      | Postgres 17, data in the `pgdata` volume                                                           |
| `backend` | Django under Daphne (ASGI); applies migrations on every start; uploads go to the `media` volume    |
| `web`     | nginx: serves the built frontend, proxies `/api/` to the backend (SSE unbuffered), serves `/api/media/` from the `media` volume |

The compose project is named `stowarzysz-prod`, so it never shares containers or volumes with the dev `docker-compose.yml`.

### Prerequisites

- A server with Docker and the Compose plugin.
- A domain name pointing at it, and a reverse proxy that terminates **HTTPS** (Caddy, Traefik, nginx + certbot, a cloud load balancer, ...). HTTPS is not optional: browsers only allow service workers and Web Push on HTTPS.

### 1. Configure

```sh
cp .env.prod.example .env.prod
```

Fill in `.env.prod`:

| Variable                                | Meaning                                                                                  |
| --------------------------------------- | ---------------------------------------------------------------------------------------- |
| `POSTGRES_PASSWORD`                     | Any strong password; the database is only reachable inside the compose network           |
| `SECRET_KEY`                            | Django secret. `python -c "import secrets; print(secrets.token_urlsafe(50))"`            |
| `SECRET_SANTA_KEY`                      | Fernet key, see [below](#secret-santa-key). Generate once and back it up                 |
| `FRONTEND_URL`                          | Public URL, e.g. `https://stowarzysz.example.com` (no trailing slash). Used for activation links and CORS |
| `ALLOWED_HOSTS`                         | Comma-separated hostnames, e.g. `stowarzysz.example.com`                                 |
| `WEB_PORT`                              | Host port for the `web` container (default `8080`)                                       |
| `GEMINI_API_KEY`                        | Optional. Google AI Studio key for receipt OCR (`GEMINI_MODEL` overrides the model)      |
| `VAPID_*`                               | Web Push keys, see [step 2](#2-generate-vapid-keys-web-push)                             |

### 2. Generate VAPID keys (Web Push)

Push notifications ("a new vote needs your answer") are sent with the Web Push protocol. The browser vendors' push services (Google, Mozilla, Apple) are shared by everyone, so your server proves who it is with a **VAPID key pair**:

- the **public key** is handed to the browser when a user enables notifications; the browser ties that subscription to it;
- the **private key** stays on the server and signs every push it sends;
- `VAPID_SUBJECT` is a contact (`mailto:you@example.com`) the push services can use if your server misbehaves.

Generate the pair once:

```sh
docker compose -f docker-compose.prod.yml --env-file .env.prod run --rm backend python manage.py generate_vapid_keys
```

Paste the printed `VAPID_PUBLIC_KEY` and `VAPID_PRIVATE_KEY` into `.env.prod` and set `VAPID_SUBJECT`.

- **Keep the keys stable.** Subscriptions are bound to the public key; if you change it, every user has to re-enable notifications. Back the keys up with the rest of `.env.prod`.
- **Optional.** Without keys the app works normally: notifications are skipped and the frontend hides the toggle.
- The backend container needs outbound HTTPS access to the push services.
- iOS only supports push for a PWA that was added to the home screen.

### Secret Santa key

`SECRET_SANTA_KEY` is a [Fernet](https://cryptography.io/en/latest/fernet/) key that encrypts who draws whom while an event is running, so the database alone never reveals the pairing. Generate it once with `python -c "from cryptography.fernet import Fernet; print(Fernet.generate_key().decode())"`. **If you lose or change it, pairings of running events can no longer be decrypted.**

### 3. Start

```sh
docker compose -f docker-compose.prod.yml --env-file .env.prod up -d --build
```

The first start builds both images and runs the database migrations automatically. The app is now listening on `http://<server>:<WEB_PORT>`.

### 4. Put HTTPS in front

Point your reverse proxy at `localhost:<WEB_PORT>`. It must pass the `Host` header through and set `X-Forwarded-Proto`. For example, with Caddy the whole config is:

```
stowarzysz.example.com {
    reverse_proxy localhost:8080
}
```

Leave response buffering off for `/api/events/` if your proxy buffers by default (the SSE stream must flow immediately).

### 5. Create the first admin

Accounts have a username only and are created by superusers in the app (users activate their account through a link), so the very first superuser is created from the command line:

```sh
docker compose -f docker-compose.prod.yml --env-file .env.prod exec backend python manage.py createsuperuser
```

Then log in at `FRONTEND_URL` and create everyone else from the management screen.

### Operations

```sh
docker compose -f docker-compose.prod.yml --env-file .env.prod logs -f backend    # logs
docker compose -f docker-compose.prod.yml --env-file .env.prod up -d --build      # update after git pull (migrations run on start)
docker compose -f docker-compose.prod.yml --env-file .env.prod exec db pg_dump -U stowarzysz stowarzysz > backup.sql   # database backup
```

Votes end automatically 72 hours after they are called, with push reminders at 24, 48 and 69 hours. The same job moves overdue pacts to "awaiting result", nudges their participants weekly and expires invites unanswered for 7 days. `scripts/deploy.sh` installs the host cron entry that runs this every 5 minutes; by hand (it is safe to run as often as you like) it is:

```sh
*/5 * * * * cd /path/to/stowarzysz && docker compose -f docker-compose.prod.yml --env-file .env.prod exec -T backend python manage.py process_deadlines
```

In development, run `uv run python manage.py process_deadlines` from `backend/` by hand.

Database shell (`psql`) in production:

```sh
docker compose -f docker-compose.prod.yml --env-file .env.prod exec db psql -U stowarzysz stowarzysz
```

Be careful: this is the live data. Prefer read-only queries, and take a backup first before changing anything.

Back up three things: the database, the `media` volume (profile pictures) and `.env.prod`. Never run `docker compose down -v` in production: `-v` deletes the volumes (your data).

Everything runs in a single backend process, which is what the in-memory SSE channels expect. If you ever scale the backend to several processes or replicas, set `EVENTSTREAM_REDIS` first (see [Backend](#backend)).

## Deploying

`master` is production: there are no tags or release branches, and there is no CI or image registry. The server builds the images itself. After pushing to `master`, run this on the server:

```sh
scripts/deploy.sh             # --dry-run prints the plan and changes nothing
```

It runs these steps and aborts at the first failure:

1. **Preflight**: tools, `.env.prod` (mode 600, required keys), a valid compose file, enough disk, a clean working tree on `master`. A second deploy can't run at the same time (lock).
2. **Pull**: `git fetch` and `git merge --ff-only origin/master` (never a hard reset).
3. **Back up the database** to `backups/db-<UTC time>-<previous commit>.dump` (`pg_dump -Fc`). The dump is checked with `pg_restore --list` first, and only a verified dump replaces an old one, so there are never more than **3** snapshots and a failed backup aborts the deploy before anything changes. The `media` volume and `.env.prod` are not included (see [Operations](#operations)).
4. **Build** the images while the old version keeps serving.
5. **Launch** with `up -d --wait`, then check `/api/health/` through nginx. Migrations run on backend start.
6. **Cron**: make sure the cron daemon is running, install the `process_deadlines` entry in your crontab (one line, safe to repeat), and run it once to prove it works.

Logs go to `logs/deploy-*.log` (the last 20 are kept) and the cron job logs to `logs/deadlines.log`. `scripts/deploy.sh --cron-only` repeats just the last step.

Before pushing, check that the release is sound:

```sh
docker compose up -d db
uv run --project backend pre-commit run --all-files   # lint, format, types
(cd backend && uv run pytest)
(cd backend && uv run python manage.py makemigrations --check --dry-run)   # no model changes without a migration
(cd frontend && pnpm build)
```

If the release adds a new setting, add it to `.env.prod.example`, mention it in the [configuration table](#1-configure) and add it to the server's `.env.prod` first, so nobody has to find out from a crash on startup. After a deploy, open `FRONTEND_URL` and check that you can log in and that a poll loads. The service worker only handles push and caches nothing (`index.html` is served `no-cache`, `/assets/` are hashed), so browsers and installed PWAs get the new frontend on their next page load.

### Rolling back

If the launch fails, the script puts the code back on the previous commit, rebuilds and checks it. It never restores the database by itself, because that would discard everything written since the snapshot. If the old version won't start because a migration already ran, the script prints the commands to restore the newest snapshot. To undo a release that already went out, push a revert to `master` and deploy again (the script only ever fast-forwards). Prefer rolling forward with a fix when the migration was purely additive.
