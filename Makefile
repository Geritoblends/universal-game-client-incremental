.PHONY: build-plugins build-host run clean

build-plugins:
	@echo "Building plugins..."
	cd plugins/tasksapp-core && cargo build --release --target wasm32-unknown-unknown
	cd plugins/tasksapp-client && cargo build --release --target wasm32-unknown-unknown

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
