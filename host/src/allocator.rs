// --- HEAP ALLOCATOR ---
#[derive(Debug, Clone, Copy)]
pub struct FreeBlock {
    pub addr: u32,
    pub size: u32,
}

pub struct HostHeap {
    pub free_blocks: Vec<FreeBlock>,
}

impl HostHeap {
    pub fn new() -> Self {
        Self {
            free_blocks: Vec::new(),
        }
    }

    pub fn coalesce(&mut self) {
        if self.free_blocks.is_empty() {
            return;
        }
        self.free_blocks.sort_by_key(|b| b.addr);
        let mut i = 0;
        while i < self.free_blocks.len() - 1 {
            let current = self.free_blocks[i];
            let next = self.free_blocks[i + 1];
            if current.addr + current.size == next.addr {
                self.free_blocks[i].size += next.size;
                self.free_blocks.remove(i + 1);
            } else {
                i += 1;
            }
        }
    }

    pub fn alloc(&mut self, size: u32) -> Option<u32> {
        if let Some(pos) = self.free_blocks.iter().position(|b| b.size >= size) {
            let block = self.free_blocks[pos];
            if block.size == size {
                self.free_blocks.remove(pos);
                Some(block.addr)
            } else {
                let ret_addr = block.addr;
                self.free_blocks[pos].addr += size;
                self.free_blocks[pos].size -= size;
                Some(ret_addr)
            }
        } else {
            None
        }
    }

    pub fn dealloc(&mut self, ptr: u32, size: u32) {
        self.free_blocks.push(FreeBlock { addr: ptr, size });
        self.coalesce();
    }
}
