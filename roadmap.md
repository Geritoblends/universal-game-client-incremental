# Universal Game Client - Development Roadmap

## Phase 1: Networked Architecture (The "Headless Browser")
*Goal: Decouple the Host from the file system. Turn the Host into a dumb terminal that executes whatever the Server sends.*

- [ ] **Workspace Restructuring**
    - [ ] Convert project to a Cargo Workspace.
    - [ ] Create `crates/shared-protocol` (Library for NetPacket enums).
    - [ ] Create `crates/asset-server` (Binary for serving Wasm).
    - [ ] Move `host` and `plugins` into the workspace structure.

- [ ] **Protocol Definition (`shared-protocol`)**
    - [ ] Define `NetPacket` enum (InstallPlugin, CallFunction, Ready).
    - [ ] Implement `bincode` serialization for packets.
    - [ ] Define `PluginManifest` struct (Name, dependencies, is_core).

- [ ] **The Asset Server**
    - [ ] Implement TCP Listener on port 8080.
    - [ ] Implement file reading logic to load `.wasm` files from disk.
    - [ ] Implement "Handshake": Send Core Wasm -> Send Client Wasm -> Send Ready.
    - [ ] Add Length-Prefixed framing (4 bytes len + payload) for TCP streams.

- [ ] **The Agnostic Host**
    - [ ] Refactor `instantiate_plugin` to accept `&[u8]` (bytes) instead of `Path`.
    - [ ] Implement TCP Client in `main.rs`.
    - [ ] Create the Event Loop: Read Packet -> Match Packet -> Execute Action.
    - [ ] Remove all hardcoded paths from `host/src/main.rs`.

## Phase 2: The Developer Experience (The SDK)
*Goal: Hide the "Wasm Hell" (allocators, linkers, externs) from the user. Plugins should look like normal Rust.*

- [ ] **Create `crates/tasksapp-sdk`**
    - [ ] Move `HostAllocator` struct and `#[global_allocator]` setup here.
    - [ ] Move `pack` and `unpack` (i64 bit-packing helpers) here.
    - [ ] Re-export common types (`tasksapp_net`, `bincode`) so plugins don't need to manage versions.

- [ ] **SDK Macros (Reduce Boilerplate)**
    - [ ] Create `#[plugin_entry]` macro to handle `extern "C"` definitions automatically.
    - [ ] Create `#[export_system]` macro to wrap functions with the `pack/unpack` logic.

- [ ] **Refactor Existing Plugins**
    - [ ] Update `tasksapp-core` to use the new SDK.
    - [ ] Update `tasksapp-client` to use the new SDK.
    - [ ] Verify `make run` still works with the clean code.

## Phase 3: Advanced Data Architecture (Moddable ECS)
*Goal: Move from "Pointer Chasing" (Minecraft style) to "Contiguous Arrays" (Bevy style) for performance.*

- [ ] **Host Columnar Memory**
    - [ ] Implement `ComponentColumn` struct in Host (base_ptr, stride, capacity).
    - [ ] Create Host Export: `get_column_ptr(component_name) -> i32`.
    - [ ] Allocate large contiguous blocks for components (instead of random heap allocs).

- [ ] **SDK Data Access**
    - [ ] Implement `Column<T>` struct in SDK.
    - [ ] Implement index math: `base + (entity_id * size_of::<T>())`.
    - [ ] Add bounds checking (optional, debug mode only).

- [ ] **Benchmarking**
    - [ ] Create a benchmark plugin spawning 10,000 entities.
    - [ ] Compare "Pointer Chasing" speed vs. "Columnar" speed.

## Phase 4: Logic Distribution (Sharding)
*Goal: The "Universal Client" concept. Different clients handle different parts of the world state.*

- [ ] **Server Routing Logic**
    - [ ] Implement a "Shard Manager" in the Asset Server.
    - [ ] Logic: `hash(task.title) % num_clients` determines which client owns the task.
    - [ ] Implement `NetPacket::RouteMessage` to forward data between clients via Server.

- [ ] **Plugin Shard Awareness**
    - [ ] Expose `get_client_id()` and `get_total_clients()` to plugins.
    - [ ] Modify `tasksapp-core` to reject tasks that don't belong to this shard.

## Phase 5: Real-World Examples (Beyond To-Do)
*Goal: Prove the engine works for actual games/apps.*

- [ ] **Example 1: Real-time Chat**
    - [ ] Test high-frequency string passing.
    - [ ] Test broadcast logic (One message -> All clients).

- [ ] **Example 2: 2D Physics Simulation (Bouncing Balls)**
    - [ ] **Crucial Test for Shared Memory ECS.**
    - [ ] Core: Calculates positions/collisions (heavy math).
    - [ ] Client: Renders the balls (reads memory, no calculation).
    - [ ] Proves "Zero-Copy" advantage (Client reads Core's physics result instantly).

- [ ] **Example 3: Voxel Data (Minecraft-lite)**
    - [ ] Test massive memory usage (16MB+ arrays).
    - [ ] Test modifying the world (Client click -> Core update -> Client render).

## Phase 6: Hardening & Polish
*Goal: Production readiness.*

- [ ] **Switch TCP to UDP**
    - [ ] Replace `TcpStream` with `quinn` (QUIC) or `renet`.
    - [ ] Implement reliable vs. unreliable channels (Snapshot data = Unreliable, Chat = Reliable).

- [ ] **Hot Reloading**
    - [ ] Implement "Unload Plugin" (Drop the Linker, free the memory offset).
    - [ ] Implement "Reload State" (Keep the Shared Heap, replace the Code).

- [ ] **Security Audit**
    - [ ] Verify `buddy-alloc` boundaries.
    - [ ] Ensure Wasm fuel metering is enabled (prevent infinite loops).
