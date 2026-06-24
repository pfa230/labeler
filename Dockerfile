FROM node:24-bookworm-slim@sha256:862263c612aa437e3037674b85419622a9d93bff80aa1eee5398dfe686375532 AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-bookworm@sha256:6d19f49541d185805745b8baa781b1fd482118c81a3154510ee18dcce985d005 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --locked
RUN mkdir -p /seed/data /seed/assets

FROM gcr.io/distroless/cc-debian12:debug@sha256:83cd6a79595b063961fc4c21814a7961e3f2741e94bfea8bde420898719e80c5 AS runtime
WORKDIR /app
COPY --from=build /app/target/release/labeler /app/labeler
COPY --chown=65532:65532 templates/ /app/templates/
COPY fonts/ /app/fonts/
COPY --from=ui /ui/dist /app/ui/dist
COPY --chown=65532:65532 --from=build /seed/data /app/data
COPY --from=build /seed/assets /app/assets
USER nonroot
EXPOSE 8080
ENV PORT=8080
ENV LABELER_DATA_DIR=/app/data
ENV LABELER_UI_DIR=/app/ui/dist
ENV LABELER_ASSETS_DIR=/app/assets
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/busybox/wget","-qO-","http://127.0.0.1:8080/api/health"]
ENTRYPOINT ["/app/labeler"]
