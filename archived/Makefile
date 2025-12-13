.PHONY: build-plugins build-host run clean check-env

CARGO_WASM := cargo +nightly build -Z build-std=std,panic_abort --target wasm32-unknown-unknown --release

check-env:
	@rustup toolchain list | grep nightly > /dev/null || echo "❌ Error: Rust nightly is missing. Run: rustup toolchain install nightly"
	@rustup component list --toolchain nightly | grep rust-src > /dev/null || echo "❌ Error: rust-src is missing. Run: rustup component add rust-src --toolchain nightly"

build-plugins:
	@echo "Building plugins with Shared Memory support..."
	cd plugins/tasksapp-core && $(CARGO_WASM)
	cd plugins/tasksapp-client && $(CARGO_WASM)

build-host:
	@echo "Building host..."
	cargo build --release

build: build-plugins build-host

run: build
	cargo run --release

clean:
	cargo clean
	cd plugins/tasksapp-core && cargo clean
	cd plugins/tasksapp-client && cargo clean

dev:
	cargo run
