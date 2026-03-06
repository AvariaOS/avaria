use core::arch::asm;
use core::mem::size_of;

#[repr(C, packed)]
pub struct Tss {
    _reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    _reserved1: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    _reserved2: u64,
    _reserved3: u16,
    pub iomap_base: u16,
}

impl Tss {
    pub const fn new() -> Self {
        Self {
            _reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            _reserved1: 0,
            ist1: 0,
            ist2: 0,
            ist3: 0,
            ist4: 0,
            ist5: 0,
            ist6: 0,
            ist7: 0,
            _reserved2: 0,
            _reserved3: 0,
            iomap_base: size_of::<Tss>() as u16,
        }
    }
}

static mut TSS: Tss = Tss::new();
static mut KERNEL_STACK: [u8; 0x4000] = [0; 0x4000];

pub fn init() {
    let stack_top = core::ptr::addr_of!(KERNEL_STACK) as u64 + 0x4000;
    unsafe {
        (*core::ptr::addr_of_mut!(TSS)).rsp0 = stack_top;
    }
}

pub fn tss_ptr() -> u64 {
    core::ptr::addr_of!(TSS) as u64
}

pub fn tss_size() -> u16 {
    (size_of::<Tss>() - 1) as u16
}

pub fn load(selector: u16) {
    unsafe {
        asm!("ltr {sel:x}", sel = in(reg) selector, options(nostack, nomem));
    }
}

pub fn set_kernel_stack(rsp0: u64) {
    unsafe {
        (*core::ptr::addr_of_mut!(TSS)).rsp0 = rsp0;
    }
}
