FROM rust:1.74 AS base

RUN cargo install cargo-chef

########################################################################

FROM base AS planner

WORKDIR /app/
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

########################################################################

FROM base AS builder

COPY --from=planner /app/recipe.json ./recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin smo-rs

########################################################################

FROM debian:bookworm-slim AS runtime

COPY --from=builder ./target/release/smo-rs ./target/release/smo-rs

ENTRYPOINT ["/target/release/smo-rs"]
EXPOSE     1027/tcp
WORKDIR    /data/
VOLUME     /data/
