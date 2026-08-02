# syntax=docker/dockerfile:1
FROM rust:alpine AS referee

RUN apk add --no-cache python3 python3-dev py3-pip musl-dev bash patchelf

WORKDIR /app

# Install Python build/test tooling into system site-packages (NOT /app),
# so it survives docker-compose's bind mount of the host tree over /app
# at container startup.
RUN pip install --break-system-packages --no-cache-dir maturin pytest

COPY . .

# Build the Rust extension as an abi3 wheel and install it into system
# site-packages — this is what makes `import semantic_version` resolve to
# OUR Rust port rather than the PyPI package, and it persists after the
# bind mount overlays /app at runtime.
RUN maturin build --release --interpreter python3 \
    && pip install --break-system-packages --no-cache-dir target/wheels/*.whl

CMD ["bash", "scripts/run_referee.sh"]
