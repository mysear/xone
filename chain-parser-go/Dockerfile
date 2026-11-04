FROM golang:1.26.1-bookworm AS build
WORKDIR /src/chain-parser-go
COPY common/protoc/golang /src/common/protoc/golang
COPY chain-parser-go/go.mod chain-parser-go/go.sum ./
RUN go mod download
COPY chain-parser-go/ .
RUN CGO_ENABLED=0 go build -trimpath -o /out/chain-parser-go ./cmd/chain-parser
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /out/chain-parser-go /usr/local/bin/chain-parser-go
COPY chain-parser-go/config /app/config
ENTRYPOINT ["chain-parser-go"]
CMD ["--config", "/app/config/config-dev.yaml"]
