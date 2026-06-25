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
RUN cargo build --release --locked
RUN mkdir -p /seed/data /seed/assets

FROM debian:trixie-slim@sha256:28de0877c2189802884ccd20f15ee41c203573bd87bb6b883f5f46362d24c5c2 AS runtime
# ca-certificates: the `ipp` printing path (reqwest 0.13 -> rustls-platform-verifier) uses the system
# trust store for `ipps://` printers. distroless bundled certs; debian-slim does not. See ADR-0029.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/labeler /app/labeler
COPY --chown=65532:65532 templates/ /app/templates/
COPY fonts/ /app/fonts/
COPY --from=ui /ui/dist /app/ui/dist
COPY --chown=65532:65532 --from=build /seed/data /app/data
COPY --from=build /seed/assets /app/assets
# Non-root by fixed UID:GID (debian-slim has no predefined `nonroot` user); the dirs above are owned to match.
USER 65532:65532
EXPOSE 8080
ENV PORT=8080
ENV LABELER_DATA_DIR=/app/data
ENV LABELER_UI_DIR=/app/ui/dist
ENV LABELER_ASSETS_DIR=/app/assets
# App-native healthcheck (no shell / wget needed in the runtime image). See ADR-0029.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/app/labeler","healthcheck"]
ENTRYPOINT ["/app/labeler"]
