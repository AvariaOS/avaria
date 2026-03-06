
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::kern::arch::x86_64::{cpu::{inb, outb}, idt, pic};
use crate::kern::serial;

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const CMD_PORT: u16 = 0x64;

const STATUS_OUTPUT_FULL: u8 = 1 << 0;
const STATUS_INPUT_FULL: u8 = 1 << 1;

const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_DISABLE_PORT1: u8 = 0xAD;
const CMD_DISABLE_PORT2: u8 = 0xA7;
const CMD_ENABLE_PORT1: u8 = 0xAE;
const CMD_SELF_TEST: u8 = 0xAA;

const KB_CMD_RESET: u8 = 0xFF;
const KB_RESPONSE_ACK: u8 = 0xFA;
const KB_RESPONSE_SELF_TEST_OK: u8 = 0xAA;

const CFG_PORT1_IRQ: u8 = 1 << 0;
const CFG_PORT1_CLOCK: u8 = 1 << 4;

const KB_IRQ: u8 = 1;
const KB_VECTOR: usize = 33;

const KB_BUF_SIZE: usize = 256;
static mut KB_BUF: [u8; KB_BUF_SIZE] = [0; KB_BUF_SIZE];
static KB_HEAD: AtomicUsize = AtomicUsize::new(0);
static KB_TAIL: AtomicUsize = AtomicUsize::new(0);

static mut SHIFT_HELD: bool = false;
static mut CTRL_HELD: bool = false;
static mut ALT_HELD: bool = false;
static mut CAPS_LOCK: bool = false;

static SCANCODE_TABLE: [u8; 128] = [
    0,   0x1B, b'1', b'2', b'3', b'4', b'5', b'6',
    b'7', b'8', b'9', b'0', b'-', b'=', 0x08, b'\t',
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i',
    b'o', b'p', b'[', b']', b'\n', 0,   b'a', b's',
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',
    b'\'', b'`', 0,   b'\\', b'z', b'x', b'c', b'v',
    b'b', b'n', b'm', b',', b'.', b'/', 0,   b'*',
    0,   b' ', 0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   b'-', 0,   0,   0,   b'+', 0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
];

static SCANCODE_TABLE_SHIFT: [u8; 128] = [
    0,   0x1B, b'!', b'@', b'#', b'$', b'%', b'^',
    b'&', b'*', b'(', b')', b'_', b'+', 0x08, b'\t',
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I',
    b'O', b'P', b'{', b'}', b'\n', 0,   b'A', b'S',
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':',
    b'"', b'~', 0,   b'|', b'Z', b'X', b'C', b'V',
    b'B', b'N', b'M', b'<', b'>', b'?', 0,   b'*',
    0,   b' ', 0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   b'-', 0,   0,   0,   b'+', 0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
    0,   0,   0,   0,   0,   0,   0,   0,
];

const SC_LSHIFT: u8 = 0x2A;
const SC_RSHIFT: u8 = 0x36;
const SC_LCTRL: u8 = 0x1D;
const SC_LALT: u8 = 0x38;
const SC_CAPSLOCK: u8 = 0x3A;

fn wait_input() {
    for _ in 0..100_000 {
        if unsafe { inb(STATUS_PORT) } & STATUS_INPUT_FULL == 0 {
            return;
        }
    }
}

fn wait_output() -> bool {
    for _ in 0..100_000 {
        if unsafe { inb(STATUS_PORT) } & STATUS_OUTPUT_FULL != 0 {
            return true;
        }
    }
    false
}

fn send_command(cmd: u8) {
    wait_input();
    unsafe { outb(CMD_PORT, cmd) };
}

fn send_data(data: u8) {
    wait_input();
    unsafe { outb(DATA_PORT, data) };
}

fn read_data() -> u8 {
    if wait_output() {
        unsafe { inb(DATA_PORT) }
    } else {
        0
    }
}

fn flush_output() {
    for _ in 0..32 {
        if unsafe { inb(STATUS_PORT) } & STATUS_OUTPUT_FULL == 0 {
            break;
        }
        unsafe { inb(DATA_PORT) };
    }
}

fn buf_push(ch: u8) {
    let head = KB_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) & (KB_BUF_SIZE - 1);
    if next != KB_TAIL.load(Ordering::Relaxed) {
        unsafe { KB_BUF[head] = ch };
        KB_HEAD.store(next, Ordering::Release);
    }
}

pub fn read_key() -> Option<u8> {
    let tail = KB_TAIL.load(Ordering::Relaxed);
    let head = KB_HEAD.load(Ordering::Acquire);
    if tail == head {
        return None;
    }
    let ch = unsafe { KB_BUF[tail] };
    KB_TAIL.store((tail + 1) & (KB_BUF_SIZE - 1), Ordering::Release);
    Some(ch)
}

pub fn has_key() -> bool {
    KB_TAIL.load(Ordering::Relaxed) != KB_HEAD.load(Ordering::Acquire)
}

pub fn poll() {
    let status = unsafe { inb(STATUS_PORT) };
    if status & STATUS_OUTPUT_FULL != 0 {
        let scancode = unsafe { inb(DATA_PORT) };
        process_scancode(scancode);
    }
}

fn process_scancode(scancode: u8) {
    if scancode == 0xE0 {
        return;
    }

    let released = scancode & 0x80 != 0;
    let code = scancode & 0x7F;

    match code {
        SC_LSHIFT | SC_RSHIFT => {
            unsafe { SHIFT_HELD = !released };
            return;
        }
        SC_LCTRL => {
            unsafe { CTRL_HELD = !released };
            return;
        }
        SC_LALT => {
            unsafe { ALT_HELD = !released };
            return;
        }
        SC_CAPSLOCK if !released => {
            unsafe { CAPS_LOCK = !CAPS_LOCK };
            return;
        }
        _ => {}
    }

    if released {
        return;
    }

    let shift = unsafe { SHIFT_HELD };
    let caps = unsafe { CAPS_LOCK };
    let base = SCANCODE_TABLE[code as usize];

    let ch = if caps && !shift {
        if base >= b'a' && base <= b'z' { base - 32 } else { base }
    } else if caps && shift {
        if base >= b'a' && base <= b'z' {
            base
        } else {
            SCANCODE_TABLE_SHIFT[code as usize]
        }
    } else if shift {
        SCANCODE_TABLE_SHIFT[code as usize]
    } else {
        base
    };

    if ch != 0 {
        buf_push(ch);
    }
}

fn keyboard_irq_handler(_ctx: &mut idt::IsrContext) {
    let scancode = unsafe { inb(DATA_PORT) };
    process_scancode(scancode);
    pic::eoi(KB_IRQ);
}

pub fn init() {
    serial::puts("ps2: init\n");

    send_command(CMD_DISABLE_PORT1);
    send_command(CMD_DISABLE_PORT2);

    flush_output();

    send_command(CMD_READ_CONFIG);
    let mut config = read_data();
    config |= CFG_PORT1_IRQ;
    config &= !CFG_PORT1_CLOCK;
    send_command(CMD_WRITE_CONFIG);
    send_data(config);

    send_command(CMD_SELF_TEST);
    let test_result = read_data();
    if test_result != 0x55 {
        serial::puts("ps2: controller self-test FAILED (0x");
        serial::hex(test_result as u64);
        serial::puts(")\n");
        return;
    }
    serial::puts("ps2: self-test OK\n");

    send_command(CMD_WRITE_CONFIG);
    send_data(config);

    send_command(CMD_ENABLE_PORT1);

    send_data(KB_CMD_RESET);
    let ack = read_data();
    if ack == KB_RESPONSE_ACK {
        let st = read_data();
        if st == KB_RESPONSE_SELF_TEST_OK {
            serial::puts("ps2: keyboard reset OK\n");
        } else {
            serial::puts("ps2: keyboard self-test returned 0x");
            serial::hex(st as u64);
            serial::puts("\n");
        }
    } else {
        serial::puts("ps2: keyboard reset no ACK (0x");
        serial::hex(ack as u64);
        serial::puts(")\n");
    }

    flush_output();

    idt::register_ctx_handler(KB_VECTOR, keyboard_irq_handler);
    pic::clear_mask(KB_IRQ);

    serial::puts("ps2: keyboard IRQ1 enabled\n");
}
