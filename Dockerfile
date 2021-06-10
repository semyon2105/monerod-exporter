FROM rust:1.52.1-alpine3.13@sha256:594905721611108f35e62cae7b8167e49a3524ae440657ba471a8bc1e18a2e72 AS builder
WORKDIR /build
COPY . .
RUN apk add musl-dev openssl-dev \
    && RUSTFLAGS='-C target-feature=-crt-static' cargo build --release

FROM alpine:3.13.5@sha256:def822f9851ca422481ec6fee59a9966f12b351c62ccb9aca841526ffaa9f748
WORKDIR /opt/monerod-exporter
COPY --from=builder /build/target/release/monerod-exporter /opt/monerod-exporter/
RUN apk add libgcc
USER 1000
EXPOSE 8080
ENTRYPOINT ["./monerod-exporter", "-c", "config.toml"]
