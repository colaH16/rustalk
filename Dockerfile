# Native CI builds target/release/rustalk on Ubuntu 24.04 for each architecture.
FROM ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY target/release/rustalk /usr/local/bin/rustalk
COPY frontend/index.html frontend/output.css frontend/alpine.min.js /app/static/
ENV LISTEN_ADDR=0.0.0.0:8080 STATIC_DIR=/app/static DB_PORT=5432 DB_NAME=cotalk \
    DB_SSLMODE=disable DB_INIT_SCHEMA=false DB_MAX_CONNECTIONS=10 \
    NATS_URL=nats://127.0.0.1:4222 NATS_SUBJECT=chat.global \
    SSE_BUFFER_CAPACITY=100 SSE_KEEPALIVE_SECONDS=15 DEPENDENCY_TIMEOUT_SECONDS=5 \
    SHUTDOWN_TIMEOUT_SECONDS=25 MAX_BODY_BYTES=1048576 TZ=UTC RUST_LOG=rustalk=info,warn
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/rustalk"]
