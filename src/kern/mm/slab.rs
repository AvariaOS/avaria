use super::buddy::BuddyAllocator;

const PAGE_SIZE: usize = 4096;
const SLAB_SIZES: [usize; 8] = [16, 32, 64, 128, 256, 512, 1024, 2048];
const MAX_SLABS_PER_SIZE: usize = 16;

struct SlabPage {
    base: usize,
    obj_size: usize,
    capacity: usize,
    bitmap: [u64; 4],
}

impl SlabPage {
    fn new(base: usize, obj_size: usize) -> Self {
        let capacity = PAGE_SIZE / obj_size;
        let capacity = if capacity > 256 { 256 } else { capacity };
        Self {
            base,
            obj_size,
            capacity,
            bitmap: [0; 4],
        }
    }

    fn alloc(&mut self) -> Option<*mut u8> {
        for word in 0..4 {
            let free = !self.bitmap[word];
            if free != 0 {
                let bit = free.trailing_zeros() as usize;
                let i = word * 64 + bit;
                if i >= self.capacity {
                    continue;
                }
                self.bitmap[word] |= 1 << bit;
                return Some((self.base + i * self.obj_size) as *mut u8);
            }
        }
        None
    }

    fn free(&mut self, ptr: *mut u8) -> bool {
        let addr = ptr as usize;
        if addr < self.base || addr >= self.base + self.capacity * self.obj_size {
            return false;
        }
        let idx = (addr - self.base) / self.obj_size;
        let word = idx / 64;
        let bit = idx % 64;
        self.bitmap[word] &= !(1 << bit);
        true
    }

    fn is_full(&self) -> bool {
        let full_words = self.capacity / 64;
        for w in 0..full_words {
            if self.bitmap[w] != u64::MAX {
                return false;
            }
        }
        let rem = self.capacity % 64;
        if rem > 0 {
            let mask = (1u64 << rem) - 1;
            if self.bitmap[full_words] & mask != mask {
                return false;
            }
        }
        true
    }

    fn is_empty(&self) -> bool {
        self.bitmap == [0; 4]
    }

    fn contains(&self, ptr: *mut u8) -> bool {
        let addr = ptr as usize;
        addr >= self.base && addr < self.base + self.capacity * self.obj_size
    }
}

struct SlabCache {
    obj_size: usize,
    pages: [Option<SlabPage>; MAX_SLABS_PER_SIZE],
    count: usize,
}

impl SlabCache {
    const fn new(obj_size: usize) -> Self {
        Self {
            obj_size,
            pages: [const { None }; MAX_SLABS_PER_SIZE],
            count: 0,
        }
    }

    fn alloc(&mut self, buddy: &mut BuddyAllocator) -> Option<*mut u8> {
        for i in 0..self.count {
            if let Some(ref mut page) = self.pages[i] {
                if !page.is_full() {
                    return page.alloc();
                }
            }
        }

        if self.count >= MAX_SLABS_PER_SIZE {
            return None;
        }

        let page_addr = buddy.alloc_one()?;
        let mut slab = SlabPage::new(page_addr, self.obj_size);
        let result = slab.alloc();
        self.pages[self.count] = Some(slab);
        self.count += 1;
        result
    }

    fn free(&mut self, ptr: *mut u8, buddy: &mut BuddyAllocator) -> bool {
        let mut found = None;
        for i in 0..self.count {
            if let Some(ref mut page) = self.pages[i] {
                if page.contains(ptr) {
                    page.free(ptr);
                    let empty = page.is_empty();
                    let base = page.base;
                    found = Some((i, empty, base));
                    break;
                }
            }
        }
        let (i, empty, base) = match found {
            Some(v) => v,
            None => return false,
        };
        if empty && self.count > 1 {
            let has_other_empty = self.pages[..self.count]
                .iter()
                .enumerate()
                .any(|(j, p)| j != i && matches!(p, Some(pg) if pg.is_empty()));
            if has_other_empty {
                buddy.free_one(base);
                self.pages[i] = None;
                self.compact(i);
            }
        }
        true
    }

    fn compact(&mut self, removed: usize) {
        if removed < self.count - 1 {
            let mut i = removed;
            while i < self.count - 1 {
                self.pages.swap(i, i + 1);
                i += 1;
            }
        }
        self.count -= 1;
    }
}

pub struct SlabAllocator {
    caches: [SlabCache; 8],
}

impl SlabAllocator {
    pub const fn new() -> Self {
        Self {
            caches: [
                SlabCache::new(SLAB_SIZES[0]),
                SlabCache::new(SLAB_SIZES[1]),
                SlabCache::new(SLAB_SIZES[2]),
                SlabCache::new(SLAB_SIZES[3]),
                SlabCache::new(SLAB_SIZES[4]),
                SlabCache::new(SLAB_SIZES[5]),
                SlabCache::new(SLAB_SIZES[6]),
                SlabCache::new(SLAB_SIZES[7]),
            ],
        }
    }

    pub fn alloc(&mut self, size: usize, buddy: &mut BuddyAllocator) -> Option<*mut u8> {
        for cache in self.caches.iter_mut() {
            if cache.obj_size >= size {
                return cache.alloc(buddy);
            }
        }
        None
    }

    pub fn free(&mut self, ptr: *mut u8, buddy: &mut BuddyAllocator) -> bool {
        for cache in self.caches.iter_mut() {
            if cache.free(ptr, buddy) {
                return true;
            }
        }
        false
    }
}
