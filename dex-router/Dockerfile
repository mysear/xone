FROM rust:1.90.0-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends clang libclang-dev && rm -rf /var/lib/apt/lists/*
COPY common/protoc/rust /src/common/protoc/rust
WORKDIR /src/dex-router
COPY dex-router/ .
RUN cargo build --release --locked --bin dex-router
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libstdc++6 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/dex-router/target/release/dex-router /usr/local/bin/dex-router
COPY dex-router/config /app/config
ENTRYPOINT ["dex-router"]
CMD ["--config", "/app/config/config-dev.yaml"]
