FROM rust:1.90.0-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends clang libclang-dev && rm -rf /var/lib/apt/lists/*
COPY common/protoc/rust /src/common/protoc/rust
WORKDIR /src/chain-parser-rust
COPY chain-parser-rust/ .
RUN cargo build --release --locked --bin chain-parser-rust
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libstdc++6 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/chain-parser-rust/target/release/chain-parser-rust /usr/local/bin/chain-parser-rust
COPY chain-parser-rust/config /app/config
ENTRYPOINT ["chain-parser-rust"]
CMD ["--config", "/app/config/config-dev.yaml"]
