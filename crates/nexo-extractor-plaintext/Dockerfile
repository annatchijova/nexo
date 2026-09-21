# Minimal extractor image: the binary and nothing else. No shell, no
# package manager, no libc even (the binary is statically linked against
# musl) — there is no second program an exploited extractor could reach
# for inside this container.
#
# Build (from repo root):
#   cargo build --release --target x86_64-unknown-linux-musl -p nexo-extractor-plaintext
#   docker build -t nexo-extractor-plaintext:local -f crates/nexo-extractor-plaintext/Dockerfile .
FROM scratch
COPY target/x86_64-unknown-linux-musl/release/nexo-extractor-plaintext /nexo-extractor-plaintext
ENTRYPOINT ["/nexo-extractor-plaintext"]
