### cheap version of Archetype ECS for The Grid Driver

#### Core Elements:

1. World
```rust
pub struct World {
  archetypes: Archetypes, // mostly Ids mapping to Tables (N:1)
  storages: Storages, // sparse components, archetype Tables, and Resources
  passport: HashMap<EntityId, (TableId, i32), // i32 for index
}
```

2. Storage
```rust
pub struct Storage {
  entities: Vec<EntityId>,
  components: HashMap<ComponentId, BlobVec>
}
```

3. BlobVec
```rust
pub struct BlobVec {
  id: ComponentId,
  bytes: Vec<u8>,
  layout: Layout
}
```

#### Systems, Access, Query, Scheduler and intermediate structs

1. SystemMeta
```rust
pub struct SystemMeta {
  id: SystemId,
  ptr: *const fn(i32),
  access: Access
}
```

2. AccessType
```rust
pub enum AccessType {
  Write,
  Read
}
```

2. Access
```rust
pub struct Access {
  component_reads_and_writes: FixedBitSet,
  component_writes: FixedBitSet,
  resource_reads_and_writes: FixedBitSet,
  resource_writes: FixedBitSet,
  archetypal: FixedBitSet, 
  

