# Minimal extractor image: the binary and nothing else. No shell, no
# package manager, no libc even (the binary is statically linked against
# musl, and flate2's rust_backend pulls in no C library) — there is no
# second program an exploited extractor could reach for inside this
# container.
#
# Build (from repo root):
#   cargo build --release --target x86_64-unknown-linux-musl -p nexo-extractor-pdf
#   docker build -t nexo-extractor-pdf:local -f crates/nexo-extractor-pdf/Dockerfile .
FROM scratch
COPY target/x86_64-unknown-linux-musl/release/nexo-extractor-pdf /nexo-extractor-pdf
ENTRYPOINT ["/nexo-extractor-pdf"]
