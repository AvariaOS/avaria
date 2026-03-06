pub struct Framebuffer {
    ptr: *mut u8,
    pub width: usize,
    pub height: usize,
    pitch: usize,
    bpp: usize,
}

unsafe impl Send for Framebuffer {}
unsafe impl Sync for Framebuffer {}

impl Framebuffer {
    pub fn new(ptr: *mut u8, width: usize, height: usize, pitch: usize, bpp: usize) -> Self {
        Self { ptr, width, height, pitch, bpp }
    }

    #[inline]
    pub fn put_pixel(&self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let off = y * self.pitch + x * self.bpp;
        unsafe {
            *self.ptr.add(off) = b;
            *self.ptr.add(off + 1) = g;
            *self.ptr.add(off + 2) = r;
            if self.bpp >= 4 {
                *self.ptr.add(off + 3) = 0xFF;
            }
        }
    }

    pub fn fill_rect(&self, x0: usize, y0: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.put_pixel(x, y, r, g, b);
            }
        }
    }

    pub fn clear(&self, r: u8, g: u8, b: u8) {
        self.fill_rect(0, 0, self.width, self.height, r, g, b);
    }
}
