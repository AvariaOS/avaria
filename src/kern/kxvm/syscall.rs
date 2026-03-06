use crate::kern::arch::x86_64::idt::{self, IsrContext};
use crate::kern::serial;

pub const SYS_EXIT: u64 = 0;
pub const SYS_SERIAL_WRITE: u64 = 1;

pub fn init() {
    idt::register_ctx_handler(0x80, syscall_dispatch);
    idt::set_gate_dpl(0x80, 3);
    serial::puts("syscall: int 0x80 registered (DPL=3)\n");
}

fn syscall_dispatch(ctx: &mut IsrContext) {
    let nr = ctx.rax;
    match nr {
        SYS_EXIT => {
            let code = ctx.rdi;
            serial::puts("syscall: exit(");
            serial::dec(code);
            serial::puts(")\n");
            ctx.cs = 0x08;
            ctx.ss = 0x10;
            ctx.rip = hlt_trampoline as *const () as u64;
            ctx.rflags = 0x202;
            static mut RETURN_STACK: [u8; 0x2000] = [0; 0x2000];
            let stack_top = core::ptr::addr_of!(RETURN_STACK) as u64 + 0x2000;
            ctx.rsp = stack_top;
        }
        SYS_SERIAL_WRITE => {
            let ptr = ctx.rdi as *const u8;
            let len = ctx.rsi as usize;
            if !ptr.is_null() && len < 4096 {
                let s = unsafe {
                    core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len))
                };
                serial::puts(s);
            }
            ctx.rax = 0;
        }
        _ => {
            serial::puts("syscall: unknown nr=");
            serial::dec(nr);
            serial::puts("\n");
            ctx.rax = u64::MAX;
        }
    }
}

fn hlt_trampoline() -> ! {
    crate::kern::serial::puts("ring3 task exited, back in kernel\n");
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, nomem)) };
    }
}
