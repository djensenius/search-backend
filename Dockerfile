# Build stage
FROM rust:1.87 as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder stage
COPY --from=builder /app/target/release/search-backend /usr/local/bin/search-backend

# Expose port
EXPOSE 3000

# Run the binary
CMD ["search-backend", "--port", "3000"]
