# syntax=docker/dockerfile:1.7

FROM node:24-bookworm-slim AS frontend
WORKDIR /workspace/frontend
COPY frontend/package.json frontend/package-lock.json frontend/tsconfig.json frontend/vite.config.ts frontend/index.html ./
COPY frontend/src ./src
RUN --mount=type=cache,target=/root/.npm \
    npm ci --prefer-offline --no-audit --no-fund && \
    npm run build

FROM rust:1.95-slim-bookworm AS build
WORKDIR /workspace
# hadolint ignore=DL3008
RUN apt-get update -qq && \
    apt-get install --no-install-recommends -y ca-certificates pkg-config build-essential && \
    rm -rf /var/lib/apt/lists /var/cache/apt/archives
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY src ./src
COPY migrations ./migrations
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target \
    cargo build --release --locked && \
    cp /workspace/target/release/rust-axum-react-starter-kit /tmp/uptime-console

FROM gcr.io/distroless/cc-debian12:nonroot

WORKDIR /workspace
COPY --from=build --chown=nonroot:nonroot /tmp/uptime-console /usr/local/bin/uptime-console
COPY --from=build --chown=nonroot:nonroot /workspace/migrations ./migrations
COPY --from=frontend --chown=nonroot:nonroot /workspace/frontend/dist ./frontend/dist

USER nonroot:nonroot
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/uptime-console"]
CMD ["serve"]
