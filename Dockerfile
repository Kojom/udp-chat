FROM rust:1.87 as builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./

COPY client ./client

RUN cargo build -p client --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/client /usr/local/bin/client

CMD ["client"]
