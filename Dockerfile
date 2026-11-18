FROM rust:1.99.1-slim-trixie AS build
WORKDIR /src
COPY wire wire
COPY server server
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked --manifest-path server/Cargo.toml && mkdir /data

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=build /src/server/target/release/bronze-sync /usr/local/bin/bronze-sync
COPY --from=build --chown=65532:65532 /data /data
ENV BRONZE_SYNC_LISTEN=0.0.0.0:8787 BRONZE_SYNC_DATA=/data
VOLUME /data
EXPOSE 8787
USER 65532:65532
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --start-interval=1s CMD ["bronze-sync", "health"]
ENTRYPOINT ["bronze-sync"]
