#![no_std]
#![no_main]

use core::arch::asm;

const SYS_EXIT: u64 = 0;
const SYS_SERIAL_WRITE: u64 = 1;

#[inline(always)]
unsafe fn syscall1(nr: u64, arg1: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "int 0x80",
            inout("rax") nr => ret,
            in("rdi") arg1,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
unsafe fn syscall2(nr: u64, arg1: u64, arg2: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "int 0x80",
            inout("rax") nr => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            options(nostack),
        );
    }
    ret
}

fn serial_print(s: &str) {
    unsafe {
        syscall2(SYS_SERIAL_WRITE, s.as_ptr() as u64, s.len() as u64);
    }
}

fn exit(code: u64) -> ! {
    unsafe {
        syscall1(SYS_EXIT, code);
    }
    loop {
        unsafe { asm!("hlt", options(nostack, nomem)) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    serial_print("Hello from ring 3 userspace!\n");
    serial_print("init: calling exit(0)\n");
    exit(0);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    serial_print("init: PANIC!\n");
    exit(1);
}
