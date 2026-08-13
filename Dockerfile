FROM node:24-bookworm-slim@sha256:3638d9a6fe4030bd716be989438248074489337ba3275657f93595428be4fc03 AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-trixie@sha256:1f0dbad1df66647807e6952d1db85d0b2bda7606cb2139d82517e4f009967376 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --locked

FROM debian:trixie-slim@sha256:3a39a0592364683e6bab97937b72cad5a8fa6dcbbee90edb3bb48c7f8e94f258 AS runtime
# ca-certificates: the `ipp` printing path (reqwest 0.13 -> rustls-platform-verifier) uses the system
# trust store for `ipps://` printers. distroless bundled certs; debian-slim does not. gosu drops the
# entrypoint from root to PUID:PGID. See ADR-0029.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates gosu \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/labeler /app/labeler
COPY fonts/ /app/fonts/
COPY --from=ui /ui/dist /app/ui/dist
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
EXPOSE 8080
ENV PORT=8080
ENV LABELER_UI_DIR=/app/ui/dist
# Homelab PUID/PGID model: the container starts as root, the entrypoint chowns the writable dirs to
# PUID:PGID (default 1000) and drops privileges via gosu. See ADR-0029.
ENV PUID=1000
ENV PGID=1000
# App-native healthcheck (no shell / wget needed). HEALTHCHECK CMD bypasses the entrypoint, so it runs
# directly; the binary just probes localhost HTTP. See ADR-0029.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/app/labeler","healthcheck"]
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
