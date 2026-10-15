# === Checks & Build ===
check:
    cargo check

clippy:
    cargo clippy --all-targets

fmt:
    cargo fmt

fix:
    cargo clippy --fix --all-targets
    cargo fmt

build:
    cargo build

build-release:
    cargo build --release

ci: fmt-check clippy test check

# === Run ===
run:
    cargo run

run-debug:
    RUST_LOG=debug cargo run

run-release:
    cargo run --release

# === Watch ===
watch:
    cargo watch -x check

watch-run:
    cargo watch -x run

# === Test ===
test:
    cargo test

test-verbose:
    cargo test -- --nocapture

# === Icons ===
# Regenerate assets/icons/icon.svg (font-free outlines) from icon.src.svg.
outline-icon:
    cargo run --example outline_icon -- assets/icons/icon.src.svg assets/icons/icon.svg

# === Nix ===
nix-build:
    nix build

nix-dev:
    nix develop

# === Install ===
install:
    cargo install --path .

# === Clean ===
clean:
    cargo clean

# === Internal helpers ===
fmt-check:
    cargo fmt --check
