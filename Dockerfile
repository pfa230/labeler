FROM node:26-bookworm-slim@sha256:3fe807a03a4436e7bc76b7e84e6861899cd75c9028ae99bc00581940141ae150 AS ui
WORKDIR /ui
COPY ui/package*.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

FROM rust:1-bookworm@sha256:19817ead3289c8c631c73df281e18b59b172f6a31f4f563290f69cddd06c30e9 AS build
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
