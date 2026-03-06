use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

static TSC_FREQ_KHZ: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
pub fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdtsc", out("eax") lo, out("edx") hi, options(nostack, nomem));
    }
    ((hi as u64) << 32) | (lo as u64)
}

#[inline(always)]
pub fn rdtscp() -> (u64, u32) {
    let lo: u32;
    let hi: u32;
    let aux: u32;
    unsafe {
        asm!("rdtscp", out("eax") lo, out("edx") hi, out("ecx") aux, options(nostack, nomem));
    }
    (((hi as u64) << 32) | (lo as u64), aux)
}

pub fn calibrate() {
    let pit_freq: u64 = 1193182;
    let target_ms: u64 = 10;
    let pit_count = (pit_freq * target_ms / 1000) as u16;

    unsafe {
        super::cpu::outb(0x43, 0x30);
        super::cpu::outb(0x40, (pit_count & 0xFF) as u8);
        super::cpu::outb(0x40, (pit_count >> 8) as u8);
    }

    let start = rdtsc();

    loop {
        unsafe { super::cpu::outb(0x43, 0x00) };
        let lo = unsafe { super::cpu::inb(0x40) } as u16;
        let hi = unsafe { super::cpu::inb(0x40) } as u16;
        let current = (hi << 8) | lo;
        if current == 0 || current > pit_count {
            break;
        }
    }

    let end = rdtsc();
    let ticks = end - start;
    let freq_khz = ticks / target_ms;
    TSC_FREQ_KHZ.store(freq_khz, Ordering::Relaxed);
}

pub fn freq_khz() -> u64 {
    TSC_FREQ_KHZ.load(Ordering::Relaxed)
}

pub fn freq_mhz() -> u64 {
    freq_khz() / 1000
}

pub fn ticks_to_ns(ticks: u64) -> u64 {
    let freq = freq_khz();
    if freq == 0 {
        return 0;
    }
    ticks * 1_000_000 / freq
}

pub fn ticks_to_us(ticks: u64) -> u64 {
    let freq = freq_khz();
    if freq == 0 {
        return 0;
    }
    ticks * 1_000 / freq
}

pub fn uptime_ms() -> u64 {
    let freq = freq_khz();
    if freq == 0 {
        return 0;
    }
    rdtsc() / freq
}
