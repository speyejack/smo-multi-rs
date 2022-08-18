FROM rust:1.63 AS builder
COPY . .
RUN cargo build --package proxy --release

FROM debian:buster-slim
COPY --from=builder ./target/release/proxy ./target/release/proxy
CMD ["/target/release/proxy"]