# The web (PWA) image: build with dx, serve with nginx. Build from frontend-rs/:
#   docker build --build-arg BUILD=$(git rev-parse --short HEAD) -t stowarzysz-web frontend-rs
FROM rust:1.99-bookworm AS build
RUN rustup target add wasm32-unknown-unknown \
    && curl -L --proto '=https' --tlsv1.2 -sSf \
       https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall -y dioxus-cli@0.7.10
WORKDIR /src
COPY . .
# Identifies this build; open tabs compare it with /version.json and offer to reload.
ARG BUILD=dev
ENV STOWARZYSZ_BUILD=$BUILD
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    cd crates/app \
    && dx bundle --platform web --release \
    && cp -r /src/target/dx/stowarzysz/release/web/public /site \
    && printf '{"build":"%s"}\n' "$BUILD" > /site/version.json

FROM nginx:1-alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /site /usr/share/nginx/html
EXPOSE 80
