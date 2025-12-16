#### Glossary

1. Host
Our WebAssembly shared memory Host.
It allows performant inter-plugin communication by using a `SharedMemory` and dynamic function linking `host_link_call`

2. Plugin
A `.wasm` file that `Host` loads.

3. The Embedder
The interface that uses `Host`. It can be an engine (unity, unreal, bevy, godot...), a CLI interface, or whatever. It can define its own host calls that instantiated plugins can execute.

### Relationships

1. Host
It doesn't know about specific plugins or embedders. It simply provides a performant shared sandbox for the plugins, and allows the embedder to define its own host calls.

2. Plugin
It's aware of its host and must stick to the specific available host calls.

3. Embedder
Must know what host calls are needed for its specific environment/ecosystem. It doesn't necessarily know about plugins.

