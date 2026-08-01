# syntax=docker/dockerfile:1
FROM rust:1.80-alpine AS builder

RUN apk add --no-cache musl-dev python3 python3-dev py3-pip

WORKDIR /build
COPY . .

# Build the Rust crate (musl target, since we're on Alpine)
RUN cargo build --release --target x86_64-unknown-linux-musl || cargo build --release

# --- Referee stage: runs the differential fuzz / test-parity harness ---
FROM rust:1.80-alpine AS referee

RUN apk add --no-cache python3 py3-pip bash

WORKDIR /app
COPY --from=builder /build /app

# Install the original Python package so its test suite has something
# to import for reference-value comparison during differential fuzzing.
RUN pip install --break-system-packages semantic-version pytest || true

CMD ["bash", "-c", "echo 'ferric-semver referee: run scripts/run_referee.sh'"]
