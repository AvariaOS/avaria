const COM1: u16 = 0x3F8;

pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x03);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xC7);
        outb(COM1 + 4, 0x0B);
    }
}

pub fn putb(b: u8) {
    unsafe {
        while (inb(COM1 + 5) & 0x20) == 0 {}
        outb(COM1, b);
    }
}

pub fn puts(s: &str) {
    for b in s.bytes() {
        putb(b);
    }
}

pub fn hex(mut v: u64) {
    if v == 0 {
        putb(b'0');
        return;
    }
    let mut buf = [0u8; 16];
    let mut i = 0usize;
    while v > 0 {
        let d = (v & 0xF) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        v >>= 4;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        putb(buf[i]);
    }
}

pub fn dec(mut v: u64) {
    if v == 0 {
        putb(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0usize;
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        putb(buf[i]);
    }
}

#[inline(always)]
unsafe fn outb(port: u16, val: u8) {
    unsafe {
        core::arch::asm!("out dx, al",
            in("dx") port, in("al") val,
            options(nostack, preserves_flags));
    }
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        core::arch::asm!("in al, dx",
            in("dx") port, out("al") val,
            options(nostack, preserves_flags));
    }
    val
}
