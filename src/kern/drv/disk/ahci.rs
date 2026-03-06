
use crate::kern::drv::pci;
use crate::kern::mem;
use crate::kern::serial;
use core::ptr;

const GHC_AE: u32 = 1 << 31;
const GHC_IE: u32 = 1 << 1;
const GHC_HR: u32 = 1 << 0;

const PORT_CMD_ST: u32 = 1 << 0;
const PORT_CMD_FRE: u32 = 1 << 4;
const PORT_CMD_FR: u32 = 1 << 14;
const PORT_CMD_CR: u32 = 1 << 15;

const PORT_TFD_BSY: u32 = 1 << 7;
const PORT_TFD_DRQ: u32 = 1 << 3;
const PORT_TFD_ERR: u32 = 1 << 0;

const SATA_SIG_ATA: u32 = 0x00000101;
const SATA_SIG_ATAPI: u32 = 0xEB140101;

const FIS_TYPE_REG_H2D: u8 = 0x27;

const ATA_CMD_IDENTIFY: u8 = 0xEC;
const ATA_CMD_READ_DMA_EXT: u8 = 0x25;

const MAX_PORTS: usize = 32;

#[repr(C)]
struct HbaMemory {
    cap: u32,
    ghc: u32,
    is: u32,
    pi: u32,
    vs: u32,
    ccc_ctl: u32,
    ccc_ports: u32,
    em_loc: u32,
    em_ctl: u32,
    cap2: u32,
    bohc: u32,
    _reserved: [u8; 0x60 - 0x2C],
    _vendor: [u8; 0x100 - 0x60],
    ports: [HbaPort; 32],
}

#[repr(C)]
struct HbaPort {
    clb: u32,
    clbu: u32,
    fb: u32,
    fbu: u32,
    is: u32,
    ie: u32,
    cmd: u32,
    _reserved0: u32,
    tfd: u32,
    sig: u32,
    ssts: u32,
    sctl: u32,
    serr: u32,
    sact: u32,
    ci: u32,
    sntf: u32,
    fbs: u32,
    _reserved1: [u32; 11],
    _vendor: [u32; 4],
}

#[repr(C)]
struct HbaCmdHeader {
    flags: u16,
    prdtl: u16,
    prdbc: u32,
    ctba: u32,
    ctbau: u32,
    _reserved: [u32; 4],
}

#[repr(C)]
struct HbaPrdtEntry {
    dba: u32,
    dbau: u32,
    _reserved: u32,
    dbc: u32,
}

#[repr(C)]
struct HbaCmdTable {
    cfis: [u8; 64],
    acmd: [u8; 16],
    _reserved: [u8; 48],
    prdt: [HbaPrdtEntry; 1],
}

#[repr(C)]
struct FisRegH2D {
    fis_type: u8,
    flags: u8,
    command: u8,
    feature_lo: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,
    lba3: u8,
    lba4: u8,
    lba5: u8,
    feature_hi: u8,
    count_lo: u8,
    count_hi: u8,
    icc: u8,
    control: u8,
    _reserved: [u8; 4],
}

static mut ABAR: *mut HbaMemory = ptr::null_mut();
static mut ACTIVE_PORTS: u32 = 0;

static mut PORT_CLB: [usize; MAX_PORTS] = [0; MAX_PORTS];
static mut PORT_FB: [usize; MAX_PORTS] = [0; MAX_PORTS];
static mut PORT_CTBA: [usize; MAX_PORTS] = [0; MAX_PORTS];
static mut IDENTIFY_BUF: *mut u8 = core::ptr::null_mut();

unsafe fn mmio_read(addr: *const u32) -> u32 {
    ptr::read_volatile(addr)
}

unsafe fn mmio_write(addr: *mut u32, val: u32) {
    ptr::write_volatile(addr, val);
}

unsafe fn port_ptr(port: usize) -> *mut HbaPort {
    &raw mut (*ABAR).ports[port]
}

unsafe fn port_read(port: usize, field: unsafe fn(*const HbaPort) -> *const u32) -> u32 {
    mmio_read(field(port_ptr(port)))
}

unsafe fn port_write(port: usize, field: unsafe fn(*mut HbaPort) -> *mut u32, val: u32) {
    mmio_write(field(port_ptr(port)) as *mut u32, val);
}

unsafe fn port_cmd(p: *const HbaPort) -> *const u32 { &raw const (*p).cmd }
unsafe fn port_cmd_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).cmd }
unsafe fn port_tfd(p: *const HbaPort) -> *const u32 { &raw const (*p).tfd }
unsafe fn port_sig(p: *const HbaPort) -> *const u32 { &raw const (*p).sig }
unsafe fn port_ssts(p: *const HbaPort) -> *const u32 { &raw const (*p).ssts }
unsafe fn port_serr_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).serr }
unsafe fn port_is_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).is }
unsafe fn port_ci(p: *const HbaPort) -> *const u32 { &raw const (*p).ci }
unsafe fn port_ci_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).ci }
unsafe fn port_clb_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).clb }
unsafe fn port_clbu_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).clbu }
unsafe fn port_fb_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).fb }
unsafe fn port_fbu_mut(p: *mut HbaPort) -> *mut u32 { &raw mut (*p).fbu }

unsafe fn stop_port(port: usize) {
    let p = port_ptr(port);
    let mut cmd = mmio_read(port_cmd(p));

    cmd &= !PORT_CMD_ST;
    mmio_write(port_cmd_mut(p), cmd);

    cmd &= !PORT_CMD_FRE;
    mmio_write(port_cmd_mut(p), cmd);

    for _ in 0..1_000_000 {
        let c = mmio_read(port_cmd(p));
        if (c & PORT_CMD_CR) == 0 && (c & PORT_CMD_FR) == 0 {
            return;
        }
        core::hint::spin_loop();
    }
    serial::puts("ahci: WARN: port stop timeout\n");
}

unsafe fn start_port(port: usize) {
    let p = port_ptr(port);

    for _ in 0..1_000_000 {
        if mmio_read(port_cmd(p)) & PORT_CMD_CR == 0 {
            break;
        }
        core::hint::spin_loop();
    }

    let mut cmd = mmio_read(port_cmd(p));
    cmd |= PORT_CMD_FRE;
    mmio_write(port_cmd_mut(p), cmd);

    cmd |= PORT_CMD_ST;
    mmio_write(port_cmd_mut(p), cmd);
}

unsafe fn wait_port_ready(port: usize) -> bool {
    let p = port_ptr(port);
    for _ in 0..1_000_000 {
        let tfd = mmio_read(port_tfd(p));
        if (tfd & (PORT_TFD_BSY | PORT_TFD_DRQ)) == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

unsafe fn issue_command(port: usize) -> bool {
    let p = port_ptr(port);

    mmio_write(port_is_mut(p), 0xFFFFFFFF);

    mmio_write(port_ci_mut(p), 1);

    for _ in 0..10_000_000 {
        let ci = mmio_read(port_ci(p));
        if ci & 1 == 0 {
            let tfd = mmio_read(port_tfd(p));
            if tfd & PORT_TFD_ERR != 0 {
                serial::puts("ahci: command error, TFD=0x");
                serial::hex(tfd as u64);
                serial::puts("\n");
                return false;
            }
            return true;
        }

        let is = mmio_read(&raw const (*p).is);
        if is & (1 << 30) != 0 {
            serial::puts("ahci: task file error\n");
            return false;
        }

        core::hint::spin_loop();
    }

    serial::puts("ahci: command timeout\n");
    false
}

unsafe fn setup_command(
    port: usize,
    command: u8,
    lba: u64,
    count: u16,
    buf_phys: u64,
    buf_size: u32,
    write: bool,
) {
    let clb = PORT_CLB[port] as *mut HbaCmdHeader;
    let ctba = PORT_CTBA[port] as *mut HbaCmdTable;

    ptr::write_bytes(ctba, 0, 1);

    let hdr = &mut *clb;
    hdr.flags = 5;
    if write {
        hdr.flags |= 1 << 6;
    }
    hdr.prdtl = 1;
    hdr.prdbc = 0;

    let prdt = &mut (*ctba).prdt[0];
    prdt.dba = buf_phys as u32;
    prdt.dbau = (buf_phys >> 32) as u32;
    prdt._reserved = 0;
    prdt.dbc = buf_size.wrapping_sub(1);

    let fis = (*ctba).cfis.as_mut_ptr() as *mut FisRegH2D;
    (*fis).fis_type = FIS_TYPE_REG_H2D;
    (*fis).flags = 0x80;
    (*fis).command = command;
    (*fis).device = 0x40;
    (*fis).lba0 = (lba & 0xFF) as u8;
    (*fis).lba1 = ((lba >> 8) & 0xFF) as u8;
    (*fis).lba2 = ((lba >> 16) & 0xFF) as u8;
    (*fis).lba3 = ((lba >> 24) & 0xFF) as u8;
    (*fis).lba4 = ((lba >> 32) & 0xFF) as u8;
    (*fis).lba5 = ((lba >> 40) & 0xFF) as u8;
    (*fis).count_lo = (count & 0xFF) as u8;
    (*fis).count_hi = ((count >> 8) & 0xFF) as u8;
    (*fis).feature_lo = 0;
    (*fis).feature_hi = 0;
    (*fis).icc = 0;
    (*fis).control = 0;
}

unsafe fn identify_device(port: usize) {
    let buf = IDENTIFY_BUF;
    if buf.is_null() {
        serial::puts("ahci: IDENTIFY buf not allocated\n");
        return;
    }
    ptr::write_bytes(buf, 0, 512);

    let buf_phys = mem::virt_to_phys(buf as usize);

    if !wait_port_ready(port) {
        serial::puts("ahci: port ");
        serial::dec(port as u64);
        serial::puts(" not ready for IDENTIFY\n");
        return;
    }

    setup_command(port, ATA_CMD_IDENTIFY, 0, 1, buf_phys, 512, false);

    if !issue_command(port) {
        serial::puts("ahci: IDENTIFY failed on port ");
        serial::dec(port as u64);
        serial::puts("\n");
        return;
    }

    let words = buf as *const u16;

    serial::puts("  model: ");
    let model_start = 27;
    let model_end = 47;
    for w in model_start..model_end {
        let val = ptr::read_volatile(words.add(w));
        let hi = (val >> 8) as u8;
        let lo = (val & 0xFF) as u8;
        if hi >= 0x20 && hi <= 0x7E {
            serial::putb(hi);
        }
        if lo >= 0x20 && lo <= 0x7E {
            serial::putb(lo);
        }
    }
    serial::puts("\n");

    let sectors = ptr::read_volatile(words.add(100)) as u64
        | (ptr::read_volatile(words.add(101)) as u64) << 16
        | (ptr::read_volatile(words.add(102)) as u64) << 32
        | (ptr::read_volatile(words.add(103)) as u64) << 48;

    let size_mb = (sectors * 512) / (1024 * 1024);
    serial::puts("  sectors: ");
    serial::dec(sectors);
    serial::puts(" (");
    serial::dec(size_mb);
    serial::puts(" MB)\n");
}

unsafe fn init_port(port: usize) -> bool {
    stop_port(port);

    let clb_virt = mem::kmalloc_aligned(4096, 4096);
    if clb_virt.is_null() {
        serial::puts("ahci: failed to alloc CLB for port ");
        serial::dec(port as u64);
        serial::puts("\n");
        return false;
    }
    ptr::write_bytes(clb_virt, 0, 4096);

    let fb_virt = mem::kmalloc_aligned(4096, 4096);
    if fb_virt.is_null() {
        serial::puts("ahci: failed to alloc FB for port ");
        serial::dec(port as u64);
        serial::puts("\n");
        return false;
    }
    ptr::write_bytes(fb_virt, 0, 4096);

    let ct_virt = mem::kmalloc_aligned(4096, 4096);
    if ct_virt.is_null() {
        serial::puts("ahci: failed to alloc CT for port ");
        serial::dec(port as u64);
        serial::puts("\n");
        return false;
    }
    ptr::write_bytes(ct_virt, 0, 4096);

    PORT_CLB[port] = clb_virt as usize;
    PORT_FB[port] = fb_virt as usize;
    PORT_CTBA[port] = ct_virt as usize;

    let clb_phys = mem::virt_to_phys(clb_virt as usize);
    let fb_phys = mem::virt_to_phys(fb_virt as usize);
    let ct_phys = mem::virt_to_phys(ct_virt as usize);

    let p = port_ptr(port);
    mmio_write(port_clb_mut(p), clb_phys as u32);
    mmio_write(port_clbu_mut(p), (clb_phys >> 32) as u32);

    mmio_write(port_fb_mut(p), fb_phys as u32);
    mmio_write(port_fbu_mut(p), (fb_phys >> 32) as u32);

    let hdr = clb_virt as *mut HbaCmdHeader;
    (*hdr).ctba = ct_phys as u32;
    (*hdr).ctbau = (ct_phys >> 32) as u32;

    mmio_write(port_serr_mut(p), 0xFFFFFFFF);

    mmio_write(port_is_mut(p), 0xFFFFFFFF);

    start_port(port);

    true
}

pub fn init() {
    let id_buf = mem::kmalloc_aligned(4096, 4096);
    if id_buf.is_null() {
        serial::puts("ahci: failed to alloc IDENTIFY buffer\n");
        return;
    }
    unsafe { IDENTIFY_BUF = id_buf; }

    let addr = match pci::find_by_class(0x01, 0x06, 0x01) {
        Some(a) => a,
        None => {
            serial::puts("ahci: no AHCI controller found\n");
            return;
        }
    };

    serial::puts("ahci: found controller at PCI ");
    serial::hex(addr.bus as u64);
    serial::puts(":");
    serial::hex(addr.dev as u64);
    serial::puts(".");
    serial::hex(addr.func as u64);
    serial::puts("\n");

    addr.enable_bus_master();

    let abar_phys = addr.bar_mem_base(5);
    if abar_phys == 0 {
        serial::puts("ahci: BAR5 is zero\n");
        return;
    }

    serial::puts("ahci: ABAR phys=0x");
    serial::hex(abar_phys);
    serial::puts("\n");

    let abar_phys_aligned = abar_phys & !0xFFF;
    let num_pages = ((abar_phys - abar_phys_aligned) as usize + 0x1100 + 0xFFF) / 0x1000;
    let mapped = unsafe { mem::map_mmio(abar_phys_aligned, num_pages) };
    if mapped == 0 {
        serial::puts("ahci: failed to map ABAR\n");
        return;
    }
    let abar_virt = (mapped + (abar_phys - abar_phys_aligned) as usize) as *mut HbaMemory;

    serial::puts("ahci: ABAR mapped at 0x");
    serial::hex(abar_virt as u64);
    serial::puts("\n");

    unsafe {
        ABAR = abar_virt;

        let vs = mmio_read(&raw const (*abar_virt).vs);
        serial::puts("ahci: version ");
        serial::dec(((vs >> 16) & 0xFFFF) as u64);
        serial::puts(".");
        serial::dec((vs & 0xFFFF) as u64);
        serial::puts("\n");

        let ghc = mmio_read(&raw const (*abar_virt).ghc);
        if ghc & GHC_AE == 0 {
            mmio_write(&raw mut (*abar_virt).ghc, ghc | GHC_AE);
        }

        let pi = mmio_read(&raw const (*abar_virt).pi);
        serial::puts("ahci: ports implemented: 0x");
        serial::hex(pi as u64);
        serial::puts("\n");

        for i in 0..MAX_PORTS {
            if pi & (1 << i) == 0 {
                continue;
            }

            let p = port_ptr(i);
            let ssts = mmio_read(port_ssts(p));
            let det = ssts & 0x0F;
            let ipm = (ssts >> 8) & 0x0F;

            if det != 3 {
                continue;
            }
            if ipm != 1 {
                continue;
            }

            let sig = mmio_read(port_sig(p));
            if sig != SATA_SIG_ATA {
                serial::puts("ahci: port ");
                serial::dec(i as u64);
                serial::puts(" non-ATA sig=0x");
                serial::hex(sig as u64);
                serial::puts(", skipping\n");
                continue;
            }

            serial::puts("ahci: port ");
            serial::dec(i as u64);
            serial::puts(" — SATA disk detected\n");

            if !init_port(i) {
                continue;
            }

            ACTIVE_PORTS |= 1 << i;

            identify_device(i);
        }

        let count = ACTIVE_PORTS.count_ones();
        serial::puts("ahci: ");
        serial::dec(count as u64);
        serial::puts(" disk(s) ready\n");
    }
}

pub fn read_sectors(port: usize, lba: u64, count: u16, buf: *mut u8) -> bool {
    if port >= MAX_PORTS {
        return false;
    }
    unsafe {
        if ACTIVE_PORTS & (1 << port) == 0 {
            return false;
        }

        if !wait_port_ready(port) {
            serial::puts("ahci: read: port not ready\n");
            return false;
        }

        let buf_phys = mem::virt_to_phys(buf as usize);
        let byte_count = if count == 0 { 65536u32 * 512 } else { count as u32 * 512 };

        setup_command(port, ATA_CMD_READ_DMA_EXT, lba, count, buf_phys, byte_count, false);
        issue_command(port)
    }
}

pub fn active_ports() -> u32 {
    unsafe { ACTIVE_PORTS }
}
