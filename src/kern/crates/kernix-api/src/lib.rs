#![no_std]

#[repr(C)]
pub struct KernixApi {
    pub serial_puts: unsafe extern "C" fn(*const u8, usize),
    pub fb_draw_str: unsafe extern "C" fn(x: usize, y: usize, *const u8, usize, fg: u32, bg: u32),
    pub fs_read: unsafe extern "C" fn(*const u8, usize, *mut *const u8, *mut usize) -> i32,
    pub tsc_read: unsafe extern "C" fn() -> u64,
    pub tsc_freq_khz: unsafe extern "C" fn() -> u64,
    pub kmalloc: unsafe extern "C" fn(usize) -> *mut u8,
    pub kfree: unsafe extern "C" fn(*mut u8, usize),
    pub preempt_disable: unsafe extern "C" fn(),
    pub preempt_enable: unsafe extern "C" fn(),
}

impl KernixApi {
    pub fn serial_print(&self, s: &str) {
        unsafe { (self.serial_puts)(s.as_ptr(), s.len()) };
    }

    pub fn draw_str(&self, x: usize, y: usize, s: &str, fg: u32, bg: u32) {
        unsafe { (self.fb_draw_str)(x, y, s.as_ptr(), s.len(), fg, bg) };
    }

    pub fn fs_read_file<'a>(&self, path: &str) -> Option<&'a [u8]> {
        let mut ptr: *const u8 = core::ptr::null();
        let mut len: usize = 0;
        let ret = unsafe { (self.fs_read)(path.as_ptr(), path.len(), &mut ptr, &mut len) };
        if ret == 0 && !ptr.is_null() {
            Some(unsafe { core::slice::from_raw_parts(ptr, len) })
        } else {
            None
        }
    }

    pub fn rdtsc(&self) -> u64 {
        unsafe { (self.tsc_read)() }
    }

    pub fn tsc_khz(&self) -> u64 {
        unsafe { (self.tsc_freq_khz)() }
    }

    pub fn ticks_to_us(&self, ticks: u64) -> u64 {
        let freq = self.tsc_khz();
        if freq == 0 { return 0; }
        ticks * 1000 / freq
    }

    pub fn ticks_to_ms(&self, ticks: u64) -> u64 {
        let freq = self.tsc_khz();
        if freq == 0 { return 0; }
        ticks / freq
    }

    pub fn alloc(&self, size: usize) -> *mut u8 {
        unsafe { (self.kmalloc)(size) }
    }

    pub fn free(&self, ptr: *mut u8, size: usize) {
        unsafe { (self.kfree)(ptr, size) };
    }

    pub fn disable_preempt(&self) {
        unsafe { (self.preempt_disable)() };
    }

    pub fn enable_preempt(&self) {
        unsafe { (self.preempt_enable)() };
    }
}
