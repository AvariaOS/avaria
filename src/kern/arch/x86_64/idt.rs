use core::arch::{asm, naked_asm};
use core::mem::size_of;

const IDT_ENTRIES: usize = 256;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn set_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = 0x08;
        self.type_attr = 0x8E;
        self.ist = 0;
        self.reserved = 0;
    }

    fn set_dpl(&mut self, dpl: u8) {
        let base = self.type_attr & 0x9F;
        self.type_attr = base | ((dpl & 3) << 5);
    }
}

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u64,
}

#[repr(C, align(16))]
struct Idt {
    entries: [IdtEntry; IDT_ENTRIES],
}

static mut IDT: Idt = Idt {
    entries: [IdtEntry::empty(); IDT_ENTRIES],
};

#[repr(C)]
pub struct IsrContext {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rbp: u64, pub rdi: u64, pub rsi: u64, pub rdx: u64,
    pub rcx: u64, pub rbx: u64, pub rax: u64,
    pub vector: u64, pub error_code: u64,
    pub rip: u64, pub cs: u64, pub rflags: u64, pub rsp: u64, pub ss: u64,
}

#[repr(C)]
pub struct InterruptFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

static mut ISR_HANDLERS: [Option<fn(u64, u64)>; IDT_ENTRIES] = [None; IDT_ENTRIES];
static mut ISR_CTX_HANDLERS: [Option<fn(&mut IsrContext)>; IDT_ENTRIES] = [None; IDT_ENTRIES];

macro_rules! isr_no_err {
    ($name:ident, $num:expr) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!(
                "push 0",
                "push {num}",
                "jmp {common}",
                num = const $num,
                common = sym isr_common,
            );
        }
    };
}

macro_rules! isr_err {
    ($name:ident, $num:expr) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!(
                "push {num}",
                "jmp {common}",
                num = const $num,
                common = sym isr_common,
            );
        }
    };
}

#[unsafe(naked)]
unsafe extern "C" fn isr_common() {
    naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rdi, rsp",
        "call {handler}",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "add rsp, 16",
        "iretq",
        handler = sym interrupt_dispatch,
    );
}

unsafe extern "C" fn interrupt_dispatch(ctx: *mut IsrContext) {
    let ctx = unsafe { &mut *ctx };
    let vec = ctx.vector as usize;

    if vec < IDT_ENTRIES {
        if let Some(handler) = unsafe { (*core::ptr::addr_of!(ISR_CTX_HANDLERS))[vec] } {
            handler(ctx);
            return;
        }
        if let Some(handler) = unsafe { (*core::ptr::addr_of!(ISR_HANDLERS))[vec] } {
            handler(ctx.vector, ctx.error_code);
            return;
        }
    }

    if vec < 32 {
        crate::kern::serial::puts("EXCEPTION #");
        crate::kern::serial::dec(vec as u64);
        crate::kern::serial::puts(" err=0x");
        crate::kern::serial::hex(ctx.error_code);
        crate::kern::serial::puts(" rip=0x");
        crate::kern::serial::hex(ctx.rip);
        crate::kern::serial::puts("\n");
    }
}

pub fn register_handler(vector: usize, handler: fn(u64, u64)) {
    if vector < IDT_ENTRIES {
        unsafe { (*core::ptr::addr_of_mut!(ISR_HANDLERS))[vector] = Some(handler) };
    }
}

pub fn register_ctx_handler(vector: usize, handler: fn(&mut IsrContext)) {
    if vector < IDT_ENTRIES {
        unsafe { (*core::ptr::addr_of_mut!(ISR_CTX_HANDLERS))[vector] = Some(handler) };
    }
}

pub fn set_gate_dpl(vector: usize, dpl: u8) {
    if vector < IDT_ENTRIES {
        unsafe {
            (*core::ptr::addr_of_mut!(IDT)).entries[vector].set_dpl(dpl);
        }
    }
}

isr_no_err!(isr0, 0);
isr_no_err!(isr1, 1);
isr_no_err!(isr2, 2);
isr_no_err!(isr3, 3);
isr_no_err!(isr4, 4);
isr_no_err!(isr5, 5);
isr_no_err!(isr6, 6);
isr_no_err!(isr7, 7);
isr_err!(isr8, 8);
isr_no_err!(isr9, 9);
isr_err!(isr10, 10);
isr_err!(isr11, 11);
isr_err!(isr12, 12);
isr_err!(isr13, 13);
isr_err!(isr14, 14);
isr_no_err!(isr15, 15);
isr_no_err!(isr16, 16);
isr_err!(isr17, 17);
isr_no_err!(isr18, 18);
isr_no_err!(isr19, 19);
isr_no_err!(isr20, 20);
isr_err!(isr21, 21);
isr_no_err!(isr22, 22);
isr_no_err!(isr23, 23);
isr_no_err!(isr24, 24);
isr_no_err!(isr25, 25);
isr_no_err!(isr26, 26);
isr_no_err!(isr27, 27);
isr_no_err!(isr28, 28);
isr_no_err!(isr29, 29);
isr_err!(isr30, 30);
isr_no_err!(isr31, 31);

isr_no_err!(isr32, 32);
isr_no_err!(isr33, 33);
isr_no_err!(isr34, 34);
isr_no_err!(isr35, 35);
isr_no_err!(isr36, 36);
isr_no_err!(isr37, 37);
isr_no_err!(isr38, 38);
isr_no_err!(isr39, 39);
isr_no_err!(isr40, 40);
isr_no_err!(isr41, 41);
isr_no_err!(isr42, 42);
isr_no_err!(isr43, 43);
isr_no_err!(isr44, 44);
isr_no_err!(isr45, 45);
isr_no_err!(isr46, 46);
isr_no_err!(isr47, 47);

isr_no_err!(isr128, 0x80);

pub fn init() {
    let handlers: [unsafe extern "C" fn(); 48] = [
        isr0, isr1, isr2, isr3, isr4, isr5, isr6, isr7,
        isr8, isr9, isr10, isr11, isr12, isr13, isr14, isr15,
        isr16, isr17, isr18, isr19, isr20, isr21, isr22, isr23,
        isr24, isr25, isr26, isr27, isr28, isr29, isr30, isr31,
        isr32, isr33, isr34, isr35, isr36, isr37, isr38, isr39,
        isr40, isr41, isr42, isr43, isr44, isr45, isr46, isr47,
    ];

    for (i, &handler) in handlers.iter().enumerate() {
        unsafe {
            (*core::ptr::addr_of_mut!(IDT)).entries[i].set_handler(handler as u64);
        }
    }

    unsafe {
        (*core::ptr::addr_of_mut!(IDT)).entries[0x80].set_handler(isr128 as *const () as u64);
    }

    let idt_ptr = IdtPtr {
        limit: (size_of::<Idt>() - 1) as u16,
        base: core::ptr::addr_of!(IDT) as u64,
    };

    unsafe {
        asm!("lidt [{}]", in(reg) &idt_ptr, options(nostack));
    }
}
