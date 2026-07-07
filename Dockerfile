# Multi-stage build — hasil akhir image kecil (~20MB)
FROM rust:1.83-slim AS builder

WORKDIR /app

# Cache dependencies dulu biar rebuild cepat
COPY Cargo.toml Cargo.lock ./
RUN mkdir src static && echo "fn main() {}" > src/main.rs && touch static/index.html \
    && cargo build --release \
    && rm -rf src

COPY src ./src
COPY static ./static
COPY migrations ./migrations
# touch biar cargo tau source berubah
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/buku_kas /usr/local/bin/buku_kas
COPY --from=builder /app/migrations /migrations

# migrations path relatif ke workdir
WORKDIR /
CMD ["buku_kas"]
