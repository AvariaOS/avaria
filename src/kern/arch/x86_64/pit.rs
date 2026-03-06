
use super::cpu::outb;

const PIT_CH0_DATA: u16 = 0x40;
const PIT_CMD: u16 = 0x43;
const PIT_FREQ: u32 = 1_193_182;

pub fn init(hz: u32) {
    let divisor = PIT_FREQ / hz;
    let divisor = if divisor > 0xFFFF { 0xFFFF_u16 } else { divisor as u16 };

    unsafe {
        outb(PIT_CMD, 0x34);
        outb(PIT_CH0_DATA, (divisor & 0xFF) as u8);
        outb(PIT_CH0_DATA, (divisor >> 8) as u8);
    }
}
