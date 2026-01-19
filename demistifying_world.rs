pub struct World {
    id: WorldId,
    pub(crate) entities: Entities,
    pub(crate) allocator: EntityAllocator,
    pub(crate) components: Components,
    pub(crate) component_ids: ComponentIds,
    pub(crate) archetypes: Archetypes,
    pub(crate) storages: Storages,
    pub(crate) bundles: Bundles,
    pub(crate) observers: Observers,
    pub(crate) removed_components: RemovedComponentMessages,
    pub(crate) change_tick: AtomicU32,
    pub(crate) last_change_tick: Tick,
    pub(crate) last_check_tick: Tick,
    pub(crate) last_trigger_id: u32,
    pub(crate) command_queue: RawCommandQueue,
}

// For optimistic locking of the entities, e.g. when mutating an Entity you first need to make sure
// its still alive
#[derive(Debug, Clone)]
pub struct Entities {
    meta: Vec<EntityMeta>,
}

#[derive(Copy, Clone, Debug)]
struct EntityMeta {
    /// The current [`EntityGeneration`] of the [`EntityIndex`].
    generation: EntityGeneration, // the 'version' field
    /// The current location of the [`EntityIndex`].
    location: Option<EntityLocation>,
    /// Location and tick of the last spawn/despawn
    spawned_or_despawned: SpawnedOrDespawned,
}

#[derive(Default, Debug)]
pub struct EntityAllocator {
    /// All the entities to reuse.
    /// This is a buffer, which contains an array of [`Entity`] ids to hand out.
    /// The next id to hand out is tracked by `free_len`.
    free: Vec<Entity>,
    /// This is continually subtracted from.
    /// If it wraps to a very large number, it will be outside the bounds of `free`,
    /// and a new index will be needed.
    free_len: AtomicUsize,
    /// This is the next "fresh" index to hand out.
    /// If there are no indices to reuse, this index, which has a generation of 0, is the next to return.
    next_index: AtomicU32,
}

#[repr(C, align(8))]
pub struct Entity {
    // Do not reorder the fields here. The ordering is explicitly used by repr(C)

    // to make this struct equivalent to a u64.
    #[cfg(target_endian = "little")]
    index: EntityIndex,

    generation: EntityGeneration,

    #[cfg(target_endian = "big")]
    index: EntityIndex,
}
