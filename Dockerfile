FROM rust:1.61-alpine as builder

WORKDIR /src

RUN apk add --no-cache openssl-dev musl-dev

COPY . .

RUN cargo build --release

FROM alpine:latest

WORKDIR /app

COPY --from=builder /src/target/release/rlsr .