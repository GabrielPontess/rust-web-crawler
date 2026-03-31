FROM rustlang/rust:nightly as builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY config ./config

RUN cargo +nightly build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/crawler ./crawler
COPY config ./config

ENV RUST_LOG=info

CMD ["./crawler"]
