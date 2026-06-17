FROM node:22-bookworm-slim@sha256:e21fc383b50d5347dc7a9f1cae45b8f4e2f0d39f7ade28e4eef7d2934522b752 AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-bookworm@sha256:19817ead3289c8c631c73df281e18b59b172f6a31f4f563290f69cddd06c30e9 AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# Pre-compile dependencies using a skeleton build to cache them as a Docker layer
RUN mkdir src && echo "fn main() {}" > src/main.rs && touch src/lib.rs
RUN cargo build --release --locked
RUN rm -rf src
COPY src/ src/
RUN touch src/main.rs src/lib.rs && cargo build --release --locked
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
