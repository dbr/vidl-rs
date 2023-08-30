# 1) Build stage
FROM rust:1.68 as builder
WORKDIR /src

# Create empty source
RUN mkdir -p src && echo "fn main(){}" > src/main.rs

# copy over dependency info
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# Build to cache dependencies
RUN cargo build --release
RUN rm src/*.rs

# copy your source tree
COPY ./src ./src
COPY ./templates ./templates
COPY ./static ./static

# build for release
RUN rm ./target/release/deps/vidl*
RUN cargo build --release

# 2) Runtime stage
# FROM debian:buster-slim
FROM python:3.9-slim-bullseye

RUN apt update && apt install -y ffmpeg

# Install runtime deps
RUN pip3 install --no-cache-dir yt-dlp

# Default config
ENV VIDL_CONFIG_DIR=/config
ENV VIDL_DOWNLOAD_DIR=/downloads

# Copy binary from build
COPY --from=builder /src/target/release/vidl /usr/local/bin

# Run
VOLUME ["/downloads", "/config"]
ENTRYPOINT ["vidl", "web", "-v"]
