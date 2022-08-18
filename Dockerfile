FROM rust:1.63 AS builder
COPY . .
RUN cargo build --release --bin smo-rs

FROM debian:buster-slim
COPY --from=builder ./target/release/smo-rs ./target/release/smo-rs
ENTRYPOINT ["/target/release/smo-rs"]
