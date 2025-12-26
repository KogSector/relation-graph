# Build stage
FROM rust:1.75-slim AS builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev libpq-dev build-essential && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy source to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release || true

# Copy actual source
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage - minimal image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates libssl3 libpq5 curl && \
    rm -rf /var/lib/apt/lists/*

# Create app user for security
RUN useradd -m -u 1001 appuser

# Copy binary from builder
COPY --from=builder /app/target/release/relation-graph /usr/local/bin/relation-graph

# Switch to non-root user
USER appuser

EXPOSE 3018

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3018/health || exit 1

CMD ["relation-graph"]
