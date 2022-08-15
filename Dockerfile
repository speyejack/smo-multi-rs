FROM rust:1.61 AS builder
COPY . .
RUN cargo build --release --bin smo-rs

FROM debian:buster-slim
COPY --from=builder ./target/release/smo-rs ./target/release/smo-rs
CMD ["/target/release/smo-rs"]
