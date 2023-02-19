FROM rust:1.67-slim as builder

RUN apt-get update && apt-get install musl-tools -y

WORKDIR /usr/src

# Create blank project
RUN USER=root cargo new ow-api

# We want dependencies cached, so copy those first.
COPY Cargo.toml Cargo.lock /usr/src/ow-api/

# Set the working directory
WORKDIR /usr/src/ow-api

## Install target platform (Cross-Compilation) --> Needed for Alpine
RUN rustup target add x86_64-unknown-linux-musl

# This is a dummy build to get the dependencies cached.
RUN cargo build --target x86_64-unknown-linux-musl --release

# Now copy in the rest of the sources
COPY src /usr/src/ow-api/src/
COPY static /usr/src/ow-api/static/

## Touch main.rs to prevent cached release build
RUN touch /usr/src/ow-api/src/main.rs

# This is the actual application build.
RUN cargo build --target x86_64-unknown-linux-musl --release


FROM alpine:3 AS runtime

# Copy application binary from builder image
COPY --from=builder /usr/src/ow-api/target/x86_64-unknown-linux-musl/release/ow-api /usr/local/bin

EXPOSE 8080

# Run the application
CMD ["/usr/local/bin/ow-api"]
