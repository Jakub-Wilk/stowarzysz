# syntax=docker/dockerfile:1
# The stowarzysz backend: one static-ish binary on a distroless base.
#   docker build -t stowarzysz-backend .
# Dependencies are cooked in their own layer (cargo-chef), so code changes rebuild only the crates.

ARG RUST_VERSION=1.99.0

FROM rust:${RUST_VERSION}-slim-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
# sqlx checks queries against the committed .sqlx/ metadata instead of a live database.
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked --bin stowarzysz

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/stowarzysz /usr/local/bin/stowarzysz
# uid 1000 owns the existing `media` volume (it was created by the previous Python image).
USER 1000:1000
ENV BIND_ADDR=0.0.0.0:8000 \
    MEDIA_ROOT=/data/media \
    LOG_FORMAT=json
EXPOSE 8000
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=6 \
    CMD ["/usr/local/bin/stowarzysz", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/stowarzysz"]
CMD ["serve"]
