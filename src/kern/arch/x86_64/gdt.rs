use core::arch::asm;
use core::mem::size_of;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct GdtEntry {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl GdtEntry {
    const fn new(base: u32, limit: u32, access: u8, flags: u8) -> Self {
        Self {
            limit_low: (limit & 0xFFFF) as u16,
            base_low: (base & 0xFFFF) as u16,
            base_mid: ((base >> 16) & 0xFF) as u8,
            access,
            granularity: ((limit >> 16) & 0x0F) as u8 | (flags << 4),
            base_high: ((base >> 24) & 0xFF) as u8,
        }
    }

    const fn null() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

#[repr(C, packed)]
struct GdtPtr {
    limit: u16,
    base: u64,
}

pub const KERNEL_CS: u16 = 0x08;
pub const KERNEL_DS: u16 = 0x10;
pub const USER_DS: u16 = 0x18 | 3;
pub const USER_CS: u16 = 0x20 | 3;
pub const TSS_SEL: u16 = 0x28;

#[repr(C, align(16))]
struct Gdt {
    entries: [GdtEntry; 7],
}

static mut GDT: Gdt = Gdt {
    entries: [
        GdtEntry::null(),
        GdtEntry::new(0, 0xFFFFF, 0x9A, 0xA),
        GdtEntry::new(0, 0xFFFFF, 0x92, 0xC),
        GdtEntry::new(0, 0xFFFFF, 0xF2, 0xC),
        GdtEntry::new(0, 0xFFFFF, 0xFA, 0xA),
        GdtEntry::null(),
        GdtEntry::null(),
    ],
};

pub fn init() {
    let gdt_ptr = GdtPtr {
        limit: (size_of::<Gdt>() - 1) as u16,
        base: core::ptr::addr_of!(GDT) as u64,
    };

    unsafe {
        asm!(
            "lgdt [{ptr}]",
            "push 0x08",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ss, ax",
            ptr = in(reg) &gdt_ptr,
            tmp = lateout(reg) _,
            out("ax") _,
        );
    }
}

pub fn install_tss(base: u64, limit: u16) {
    let limit_val = limit as u32;
    let base_lo = base as u32;
    let base_hi = (base >> 32) as u32;

    let entry_low = GdtEntry {
        limit_low: (limit_val & 0xFFFF) as u16,
        base_low: (base_lo & 0xFFFF) as u16,
        base_mid: ((base_lo >> 16) & 0xFF) as u8,
        access: 0x89,
        granularity: ((limit_val >> 16) & 0x0F) as u8,
        base_high: ((base_lo >> 24) & 0xFF) as u8,
    };

    let entry_high = GdtEntry {
        limit_low: (base_hi & 0xFFFF) as u16,
        base_low: ((base_hi >> 16) & 0xFFFF) as u16,
        base_mid: 0,
        access: 0,
        granularity: 0,
        base_high: 0,
    };

    unsafe {
        (*core::ptr::addr_of_mut!(GDT)).entries[5] = entry_low;
        (*core::ptr::addr_of_mut!(GDT)).entries[6] = entry_high;
    }
}
