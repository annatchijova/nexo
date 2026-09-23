# Minimal extractor image: the binary and nothing else. No shell, no
# package manager, no libc even (the binary is statically linked against
# musl) — there is no second program an exploited extractor could reach
# for inside this container.
#
# Build (from repo root):
#   cargo build --release --target x86_64-unknown-linux-musl -p nexo-extractor-eml
#   docker build -t nexo-extractor-eml:local -f crates/nexo-extractor-eml/Dockerfile .
FROM scratch
COPY target/x86_64-unknown-linux-musl/release/nexo-extractor-eml /nexo-extractor-eml
ENTRYPOINT ["/nexo-extractor-eml"]
