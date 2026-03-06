use core::arch::asm;
use super::cpu;

pub fn init() {
    if !cpu::has_feature_sse() {
        return;
    }

    unsafe {
        let mut cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0, options(nostack, nomem));
        cr0 &= !(1 << 2);
        cr0 |= 1 << 1;
        asm!("mov cr0, {}", in(reg) cr0, options(nostack));

        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nostack, nomem));
        cr4 |= 1 << 9;
        cr4 |= 1 << 10;
        if cpu::has_feature_avx() {
            cr4 |= 1 << 18;
        }
        asm!("mov cr4, {}", in(reg) cr4, options(nostack));
    }

    if cpu::has_feature_avx() {
        unsafe {
            let xcr0: u64 = 0x7;
            let lo = xcr0 as u32;
            let hi = (xcr0 >> 32) as u32;
            asm!(
                "xsetbv",
                in("ecx") 0u32,
                in("eax") lo,
                in("edx") hi,
                options(nostack, nomem),
            );
        }
    }
}
