const PAGE_SIZE: usize = 4096;
const MAX_ORDER: usize = 20;
const BITMAP_SIZE: usize = 1 << MAX_ORDER;

pub struct BuddyAllocator {
    base: usize,
    total_pages: usize,
    bitmap: [u8; BITMAP_SIZE / 8],
    max_order: usize,
}

impl BuddyAllocator {
    pub const fn new() -> Self {
        Self {
            base: 0,
            total_pages: 0,
            bitmap: [0; BITMAP_SIZE / 8],
            max_order: 0,
        }
    }

    pub fn init(&mut self, base: usize, size: usize) {
        self.base = base;
        self.total_pages = size / PAGE_SIZE;
        let mut order = 0;
        while (1 << order) < self.total_pages && order < MAX_ORDER {
            order += 1;
        }
        self.max_order = order;
        for b in self.bitmap.iter_mut() {
            *b = 0;
        }
    }

    fn bit_index(&self, order: usize, idx: usize) -> usize {
        (1 << order) - 1 + idx
    }

    fn get_bit(&self, bit: usize) -> bool {
        if bit / 8 >= self.bitmap.len() {
            return true;
        }
        self.bitmap[bit / 8] & (1 << (bit % 8)) != 0
    }

    fn set_bit(&mut self, bit: usize) {
        if bit / 8 < self.bitmap.len() {
            self.bitmap[bit / 8] |= 1 << (bit % 8);
        }
    }

    fn clear_bit(&mut self, bit: usize) {
        if bit / 8 < self.bitmap.len() {
            self.bitmap[bit / 8] &= !(1 << (bit % 8));
        }
    }

    pub fn alloc_pages(&mut self, order: usize) -> Option<usize> {
        if order > self.max_order {
            return None;
        }

        if let Some(idx) = self.find_free(order) {
            self.mark_allocated(order, idx);
            let page = idx << order;
            return Some(self.base + page * PAGE_SIZE);
        }

        let mut split_order = order + 1;
        while split_order <= self.max_order {
            if let Some(idx) = self.find_free(split_order) {
                self.mark_allocated(split_order, idx);
                let mut current_order = split_order;
                let mut current_idx = idx;
                while current_order > order {
                    current_order -= 1;
                    let buddy_idx = current_idx * 2 + 1;
                    self.mark_free(current_order, buddy_idx);
                    current_idx *= 2;
                }
                let page = current_idx << order;
                return Some(self.base + page * PAGE_SIZE);
            }
            split_order += 1;
        }

        None
    }

    pub fn free_pages(&mut self, addr: usize, order: usize) {
        if addr < self.base {
            return;
        }
        let page = (addr - self.base) / PAGE_SIZE;
        let mut idx = page >> order;
        let mut current_order = order;

        self.mark_free(current_order, idx);

        while current_order < self.max_order {
            let buddy = idx ^ 1;
            if self.is_free(current_order, buddy) {
                self.mark_allocated(current_order, idx);
                self.mark_allocated(current_order, buddy);
                idx >>= 1;
                current_order += 1;
                self.mark_free(current_order, idx);
            } else {
                break;
            }
        }
    }

    pub fn alloc_one(&mut self) -> Option<usize> {
        self.alloc_pages(0)
    }

    pub fn free_one(&mut self, addr: usize) {
        self.free_pages(addr, 0);
    }

    fn find_free(&self, order: usize) -> Option<usize> {
        let count = self.total_pages >> order;
        for idx in 0..count {
            if self.is_free(order, idx) {
                return Some(idx);
            }
        }
        None
    }

    fn is_free(&self, order: usize, idx: usize) -> bool {
        if order == self.max_order {
            let bit = self.bit_index(order, idx);
            return !self.get_bit(bit);
        }
        let bit = self.bit_index(order, idx);
        !self.get_bit(bit)
    }

    fn mark_allocated(&mut self, order: usize, idx: usize) {
        let bit = self.bit_index(order, idx);
        self.set_bit(bit);
    }

    fn mark_free(&mut self, order: usize, idx: usize) {
        let bit = self.bit_index(order, idx);
        self.clear_bit(bit);
    }

    pub fn total_pages(&self) -> usize {
        self.total_pages
    }
}
