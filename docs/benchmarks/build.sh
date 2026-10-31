#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
GOTOOLCHAIN=go1.25.5 go build -o micro-go micro/go.go
GOTOOLCHAIN=go1.25.3 go build -o client client.go
cargo build --locked --release --manifest-path micro/Cargo.toml
