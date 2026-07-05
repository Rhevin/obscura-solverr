# syntax=docker/dockerfile:1

FROM rust:1-slim-trixie AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        curl \
        ca-certificates \
        perl \
        git \
        cmake \
        pkg-config \
        clang \
        build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependency compilation by copying manifests first
COPY Cargo.toml Cargo.lock ./
COPY crates/obscura-dom/Cargo.toml       crates/obscura-dom/Cargo.toml
COPY crates/obscura-net/Cargo.toml       crates/obscura-net/Cargo.toml
COPY crates/obscura-browser/Cargo.toml   crates/obscura-browser/Cargo.toml
COPY crates/obscura-cdp/Cargo.toml       crates/obscura-cdp/Cargo.toml
COPY crates/obscura-js/Cargo.toml        crates/obscura-js/Cargo.toml
COPY crates/obscura-mcp/Cargo.toml       crates/obscura-mcp/Cargo.toml
COPY crates/obscura-solverr/Cargo.toml crates/obscura-solverr/Cargo.toml
COPY crates/obscura-cli/Cargo.toml       crates/obscura-cli/Cargo.toml

# Create stub src files so cargo can resolve the dependency graph
RUN for crate in obscura-dom obscura-net obscura-browser obscura-cdp obscura-js obscura-mcp obscura-solverr; do \
        mkdir -p crates/$crate/src && echo "// stub" > crates/$crate/src/lib.rs; \
    done && \
    mkdir -p crates/obscura-cli/src && \
    echo "fn main() {}" > crates/obscura-cli/src/main.rs && \
    echo "fn main() {}" > crates/obscura-cli/src/worker.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/build/target,sharing=locked \
    cargo build --release --bin obscura --bin obscura-worker --features stealth 2>/dev/null || true

ARG OBSCURA_VERSION

# Copy real sources and build
COPY crates/ crates/
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/build/target,sharing=locked \
    echo "Building Obscura version ${OBSCURA_VERSION:-from Cargo.toml}" && \
    touch crates/*/src/*.rs && cargo build --release --bin obscura --bin obscura-worker --features stealth && \
    cp /build/target/release/obscura /build/target/release/obscura-worker /tmp/

# ---

# distroless/cc: glibc + libgcc + CA certs only — no shell, no package manager
FROM gcr.io/distroless/cc-debian13

COPY --from=builder /tmp/obscura /obscura
COPY --from=builder /tmp/obscura-worker /obscura-worker

EXPOSE 9222 8191

# Bind to 0.0.0.0 so the port is reachable via `docker run -p 9222:9222`.
# Native binary still defaults to 127.0.0.1 (loopback only) — this override
# is just for the container.
ENTRYPOINT ["/obscura"]
CMD ["serve", "--port", "9222", "--host", "0.0.0.0", "--stealth"]
