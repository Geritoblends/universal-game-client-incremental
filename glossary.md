#### Glossary

1. Host
Our WebAssembly shared memory Host.
It allows performant inter-plugin communication by using a `SharedMemory` and dynamic function linking `host_link_call`

2. Plugin
A `.wasm` file that `Host` loads.

3. The Embedder
The interface that uses `Host`. It can be an engine (unity, unreal, bevy, godot...), a CLI interface, or whatever. It can define its own host calls that instantiated plugins can execute.

4. The Driver
A Plugin that communicates directly with The Embedder via wasm exports/host calls. It defines the full API for plugins to read Input and write Output, for example in ECS it would provide the API to register Resources, Components and Systems. Since wasm shared memory communication is via raw pointers, unsafe blocks and `host_link_call` calls, The Driver needs The Driver Library to make everything a safe and ergonomic abstraction

5. The Canonical Input
The input (keyboard, mouse, whatever) that The Embedder providers to The Driver and hence The Plugins. In ECS, this would be a Resource plugins can only read

6. The Canonical Output
The output (mostly audio or the visual state of the game) that The Driver (and hence the Plugins) provides to The Embedder. In ECS, this would also be a Resource plugins can mutate

7. The Protocol
A protocol crate that both The Embedder and The Driver use to communicate Input and Output

8. The Driver Library
A library crate that Plugins use to interact with The Driver, for example in ECS, it would make the creation of Resources, Components and Systems more ergonomic, by wrapping The Driver raw exports into more comfortable methods/macros.

### Relationships

1. Host
It doesn't know about specific plugins or embedders. It simply provides a performant shared sandbox for the plugins, allows the embedder to define its own host calls, and call explicit wasm exports.

2. Plugin
It's aware of its host and must stick to the specific available host calls.

3. Embedder
Must know what host calls/wasm exports from The Driver are needed for its specific environment/ecosystem. It doesn't necessarily know about plugins besides The Driver

