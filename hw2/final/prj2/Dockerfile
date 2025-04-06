# === Build Stage ===
FROM rust:1.70 AS builder
WORKDIR /app

# Cache dependencies by copying Cargo manifests first.
COPY Cargo.toml ./

# Create a dummy main file to allow dependency caching.
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy the full source code and build the real binary.
COPY . .
RUN cargo build --release

# === Runtime Stage ===
FROM ubuntu:22.04
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage.
# Rename the binary to "peer" to match your ENTRYPOINT.
COPY --from=builder /app/target/release/part1 /app/peer

# Copy the hosts file into the image.
COPY hostsfile.txt /app/hostsfile.txt

# Set the entrypoint to the compiled binary.
ENTRYPOINT ["/app/peer"]