FROM python:3.14-slim

COPY --from=ghcr.io/astral-sh/uv:0.12.24 /uv /usr/local/bin/uv

ENV UV_COMPILE_BYTECODE=1 \
    UV_LINK_MODE=copy \
    UV_PROJECT_ENVIRONMENT=/opt/venv \
    PATH="/opt/venv/bin:$PATH" \
    PYTHONUNBUFFERED=1

WORKDIR /app

COPY pyproject.toml uv.lock ./
RUN --mount=type=cache,target=/root/.cache/uv \
    uv sync --locked --no-dev --no-install-project

COPY . .

RUN useradd --system --uid 1000 app && mkdir -p /data/media && chown app /data/media
USER app

ENV MEDIA_ROOT=/data/media
EXPOSE 8000

# Apply migrations, then serve over ASGI.
CMD ["sh", "-c", "python manage.py migrate --noinput && exec daphne -b 0.0.0.0 -p 8000 config.asgi:application"]
