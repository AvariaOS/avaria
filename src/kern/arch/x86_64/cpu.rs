use core::arch::asm;

#[inline(always)]
pub unsafe fn outb(port: u16, val: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") val, options(nostack, preserves_flags));
    }
}

#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") val, options(nostack, preserves_flags));
    }
    val
}

#[inline(always)]
pub unsafe fn outw(port: u16, val: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") val, options(nostack, preserves_flags));
    }
}

#[inline(always)]
pub unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe {
        asm!("in ax, dx", in("dx") port, out("ax") val, options(nostack, preserves_flags));
    }
    val
}

#[inline(always)]
pub unsafe fn outl(port: u16, val: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") val, options(nostack, preserves_flags));
    }
}

#[inline(always)]
pub unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") val, options(nostack, preserves_flags));
    }
    val
}

#[inline(always)]
pub fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nostack, nomem));
    }
    ((hi as u64) << 32) | (lo as u64)
}

#[inline(always)]
pub fn wrmsr(msr: u32, val: u64) {
    let lo = val as u32;
    let hi = (val >> 32) as u32;
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") lo, in("edx") hi, options(nostack, nomem));
    }
}

#[inline(always)]
pub fn cli() {
    unsafe { asm!("cli", options(nostack, nomem)) };
}

#[inline(always)]
pub fn sti() {
    unsafe { asm!("sti", options(nostack, nomem)) };
}

#[inline(always)]
pub fn save_irq() -> u64 {
    let flags: u64;
    unsafe {
        asm!("pushfq; pop {}; cli", out(reg) flags, options(nomem));
    }
    flags
}

#[inline(always)]
pub fn restore_irq(flags: u64) {
    if flags & 0x200 != 0 {
        sti();
    }
}

#[inline(always)]
pub fn hlt() {
    unsafe { asm!("hlt", options(nostack, nomem)) };
}

#[inline(always)]
pub fn pause() {
    unsafe { asm!("pause", options(nostack, nomem)) };
}

pub fn cpuid(leaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    unsafe {
        asm!(
            "mov r8d, ebx",
            "cpuid",
            "xchg r8d, ebx",
            inout("eax") leaf => eax,
            out("r8d") ebx,
            inout("ecx") 0u32 => ecx,
            out("edx") edx,
            options(nostack, nomem),
        );
    }
    (eax, ebx, ecx, edx)
}

pub fn cpu_vendor() -> [u8; 12] {
    let (_, ebx, ecx, edx) = cpuid(0);
    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&ecx.to_le_bytes());
    vendor
}

pub fn has_feature_sse() -> bool {
    let (_, _, _, edx) = cpuid(1);
    edx & (1 << 25) != 0
}

pub fn has_feature_sse2() -> bool {
    let (_, _, _, edx) = cpuid(1);
    edx & (1 << 26) != 0
}

pub fn has_feature_sse3() -> bool {
    let (_, _, ecx, _) = cpuid(1);
    ecx & (1 << 0) != 0
}

pub fn has_feature_sse41() -> bool {
    let (_, _, ecx, _) = cpuid(1);
    ecx & (1 << 19) != 0
}

pub fn has_feature_sse42() -> bool {
    let (_, _, ecx, _) = cpuid(1);
    ecx & (1 << 20) != 0
}

pub fn has_feature_avx() -> bool {
    let (_, _, ecx, _) = cpuid(1);
    ecx & (1 << 28) != 0
}

pub fn has_tsc() -> bool {
    let (_, _, _, edx) = cpuid(1);
    edx & (1 << 4) != 0
}

pub fn has_invariant_tsc() -> bool {
    let (max, _, _, _) = cpuid(0x80000000);
    if max < 0x80000007 {
        return false;
    }
    let (_, _, _, edx) = cpuid(0x80000007);
    edx & (1 << 8) != 0
}

pub fn lapic_id() -> u32 {
    let (_, ebx, _, _) = cpuid(1);
    ebx >> 24
}
