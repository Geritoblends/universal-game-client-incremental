build:
	@echo "Building ECS Core (Wasm)..."
	cargo +nightly build \
		-Z build-std=std,panic_abort \
		-p ecs-core \
		--target wasm32-unknown-unknown \
		--release
	
	@echo "Building My Game (Wasm)..."
	cargo +nightly build \
		-Z build-std=std,panic_abort \
		-p my-game \
		--target wasm32-unknown-unknown \
		--release

run: build
	@echo "Running Host (Native)..."
	cargo run --release -p host

clean:
	cargo clean
