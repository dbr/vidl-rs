set -e

docker build -f Dockerfile.coverage  -t vidl-cov .
docker run -it --rm --security-opt seccomp=unconfined \
    -v $PWD:/src \
    -v /tmp/vidl-docker-build:/build \
    -v /tmp/vidl-docker-cargo/registry:/usr/local/cargo/registry \
    -v /tmp/vidl-docker-cargo/git:/usr/local/cargo/git \
    vidl-cov \
    sh -c 'cd /src && env CARGO_TARGET_DIR=/build RUSTFLAGS="-C link-dead-code" cargo test --no-run && env CARGO_TARGET_DIR=/build cargo kcov --no-clean-rebuild'
