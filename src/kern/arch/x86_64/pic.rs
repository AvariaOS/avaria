use super::cpu::{inb, outb};

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_INIT: u8 = 0x11;
const ICW4_8086: u8 = 0x01;

pub fn remap(offset1: u8, offset2: u8) {
    let mask1 = unsafe { inb(PIC1_DATA) };
    let mask2 = unsafe { inb(PIC2_DATA) };

    unsafe {
        outb(PIC1_CMD, ICW1_INIT);
        io_wait();
        outb(PIC2_CMD, ICW1_INIT);
        io_wait();

        outb(PIC1_DATA, offset1);
        io_wait();
        outb(PIC2_DATA, offset2);
        io_wait();

        outb(PIC1_DATA, 4);
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();

        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        outb(PIC1_DATA, mask1);
        outb(PIC2_DATA, mask2);
    }
}

pub fn disable() {
    unsafe {
        outb(PIC1_DATA, 0xFF);
        outb(PIC2_DATA, 0xFF);
    }
}

pub fn eoi(irq: u8) {
    if irq >= 8 {
        unsafe { outb(PIC2_CMD, 0x20) };
    }
    unsafe { outb(PIC1_CMD, 0x20) };
}

pub fn set_mask(irq: u8) {
    let (port, irq) = if irq < 8 {
        (PIC1_DATA, irq)
    } else {
        (PIC2_DATA, irq - 8)
    };
    let val = unsafe { inb(port) } | (1 << irq);
    unsafe { outb(port, val) };
}

pub fn clear_mask(irq: u8) {
    let (port, irq) = if irq < 8 {
        (PIC1_DATA, irq)
    } else {
        (PIC2_DATA, irq - 8)
    };
    let val = unsafe { inb(port) } & !(1 << irq);
    unsafe { outb(port, val) };
}

#[inline(always)]
fn io_wait() {
    unsafe { outb(0x80, 0) };
}
