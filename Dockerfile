FROM node:24-bookworm-slim@sha256:862263c612aa437e3037674b85419622a9d93bff80aa1eee5398dfe686375532 AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-trixie@sha256:6df234c1eb92b0545468fab8c18fc5f9adfb994e7d4f67d81d45fe2fcabf5657 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY templates/ templates/
RUN cargo build --release --locked

FROM debian:trixie-slim@sha256:28de0877c2189802884ccd20f15ee41c203573bd87bb6b883f5f46362d24c5c2 AS runtime
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
