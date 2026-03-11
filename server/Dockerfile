# ─── Stage 1: Build ───────────────────────────────────────────────────────────
FROM rust:1.77-slim AS builder

# Dipendenze di sistema necessarie per sqlx + sqlite
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copia prima solo i file di dipendenze per sfruttare la cache Docker
COPY Cargo.toml Cargo.lock ./

# Crea una build dummy per cachare le dipendenze
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release
RUN rm -rf src

# Ora copia il codice vero
COPY src ./src

# Copia eventuali migrations e file .env
COPY migrations ./migrations

# Build finale (touch per forzare ricompilazione)
RUN touch src/main.rs && cargo build --release

# ─── Stage 2: Runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# Dipendenze runtime minime
RUN apt-get update && apt-get install -y \
    libssl3 \
    libsqlite3-0 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copia il binario compilato
COPY --from=builder /app/target/release/server .

# Copia migrations (necessarie per sqlx migrate a runtime)
COPY --from=builder /app/migrations ./migrations

# Crea la directory per il database SQLite
RUN mkdir -p /app/data

# Esponi la porta (cambia se usi una porta diversa)
EXPOSE 8080

# Avvia il server
CMD ["./server"]
