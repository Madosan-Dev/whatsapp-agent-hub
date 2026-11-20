# ============================================================================
# Stage 1: Builder (Rust 1.82 Slim)
# ============================================================================
FROM rust:1.82-slim AS builder

WORKDIR /usr/src/app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
COPY static ./static

RUN cargo build --release

# ============================================================================
# Stage 2: Minimal Hardened Runtime (Debian Bookworm Slim)
# ============================================================================
FROM debian:bookworm-slim AS runner

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system appgroup \
    && useradd --system --gid appgroup --create-home appuser \
    && mkdir -p /app/data \
    && chown -R appuser:appgroup /app

COPY --from=builder /usr/src/app/target/release/whatsapp-agent-hub /app/whatsapp-agent-hub
COPY --from=builder /usr/src/app/static /app/static
COPY --from=builder /usr/src/app/templates /app/templates

RUN chown -R appuser:appgroup /app

USER appuser

ENV PORT=3000 \
    DATABASE_URL=sqlite:/app/data/hub.db?mode=rwc \
    RUST_LOG=whatsapp_agent_hub=info,tower_http=info

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/health || exit 1

CMD ["/app/whatsapp-agent-hub"]
