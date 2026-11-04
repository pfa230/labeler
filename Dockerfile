FROM node:24-trixie-slim@sha256:54f84b4cf8432b21aaffd5d3ce8d41e12cea0d10d33c09d2f3c9879d49d6b446 AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-trixie@sha256:e259494b14672950a909b4c54527e19d29cae6ebf980672114c8bb85e5594e4a AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --locked

FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4 AS runtime
# ca-certificates: the `ipp` printing path (reqwest 0.13 -> rustls-platform-verifier) uses the system
# trust store for `ipps://` printers. distroless bundled certs; debian-slim does not. gosu drops the
# entrypoint from root to PUID:PGID.
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
# PUID:PGID (default 1000) and drops privileges via gosu.
ENV PUID=1000
ENV PGID=1000
# App-native healthcheck (no shell / wget needed). HEALTHCHECK CMD bypasses the entrypoint, so it runs
# directly; the binary just probes localhost HTTP.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/app/labeler","healthcheck"]
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
