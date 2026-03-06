use crate::kern::arch::x86_64::cpu::{inl, outl};
use crate::kern::serial;

const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

#[derive(Clone, Copy)]
pub struct PciAddr {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
}

impl PciAddr {
    pub fn config_read32(&self, offset: u8) -> u32 {
        let addr = 0x8000_0000
            | ((self.bus as u32) << 16)
            | ((self.dev as u32) << 11)
            | ((self.func as u32) << 8)
            | ((offset as u32) & 0xFC);
        unsafe {
            outl(PCI_CONFIG_ADDR, addr);
            inl(PCI_CONFIG_DATA)
        }
    }

    pub fn config_write32(&self, offset: u8, val: u32) {
        let addr = 0x8000_0000
            | ((self.bus as u32) << 16)
            | ((self.dev as u32) << 11)
            | ((self.func as u32) << 8)
            | ((offset as u32) & 0xFC);
        unsafe {
            outl(PCI_CONFIG_ADDR, addr);
            outl(PCI_CONFIG_DATA, val);
        }
    }

    pub fn config_read16(&self, offset: u8) -> u16 {
        let val = self.config_read32(offset & 0xFC);
        (val >> (((offset & 2) as u32) * 8)) as u16
    }

    pub fn config_read8(&self, offset: u8) -> u8 {
        let val = self.config_read32(offset & 0xFC);
        (val >> (((offset & 3) as u32) * 8)) as u8
    }

    pub fn vendor_id(&self) -> u16 {
        self.config_read16(0x00)
    }

    pub fn device_id(&self) -> u16 {
        self.config_read16(0x02)
    }

    pub fn class_code(&self) -> u8 {
        self.config_read8(0x0B)
    }

    pub fn subclass(&self) -> u8 {
        self.config_read8(0x0A)
    }

    pub fn prog_if(&self) -> u8 {
        self.config_read8(0x09)
    }

    pub fn header_type(&self) -> u8 {
        self.config_read8(0x0E)
    }

    pub fn interrupt_line(&self) -> u8 {
        self.config_read8(0x3C)
    }

    pub fn interrupt_pin(&self) -> u8 {
        self.config_read8(0x3D)
    }

    pub fn bar(&self, index: u8) -> u32 {
        self.config_read32(0x10 + index * 4)
    }

    pub fn bar_mem_base(&self, index: u8) -> u64 {
        let bar0 = self.bar(index);
        if bar0 & 1 != 0 {
            return (bar0 & !0x3) as u64;
        }
        let bar_type = (bar0 >> 1) & 0x3;
        let base_lo = (bar0 & !0xF) as u64;
        if bar_type == 2 {
            let bar1 = self.bar(index + 1) as u64;
            base_lo | (bar1 << 32)
        } else {
            base_lo
        }
    }

    pub fn enable_bus_master(&self) {
        let cmd = self.config_read16(0x04);
        let new_cmd = cmd | 0x06;
        let full = self.config_read32(0x04);
        self.config_write32(0x04, (full & 0xFFFF0000) | new_cmd as u32);
    }
}

pub struct PciDevice {
    pub addr: PciAddr,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

const MAX_PCI_DEVICES: usize = 64;
static mut PCI_DEVICES: [Option<PciDevice>; MAX_PCI_DEVICES] = {
    const NONE: Option<PciDevice> = None;
    [NONE; MAX_PCI_DEVICES]
};
static mut PCI_COUNT: usize = 0;

pub fn scan() {
    let mut count = 0usize;
    for bus in 0..=255u16 {
        for dev in 0..32u8 {
            let addr = PciAddr { bus: bus as u8, dev, func: 0 };
            let vendor = addr.vendor_id();
            if vendor == 0xFFFF {
                continue;
            }
            let header = addr.header_type();
            let max_func: u8 = if header & 0x80 != 0 { 8 } else { 1 };

            for func in 0..max_func {
                let faddr = PciAddr { bus: bus as u8, dev, func };
                let vid = faddr.vendor_id();
                if vid == 0xFFFF {
                    continue;
                }
                let did = faddr.device_id();
                let class = faddr.class_code();
                let subclass = faddr.subclass();
                let prog_if = faddr.prog_if();

                if count < MAX_PCI_DEVICES {
                    unsafe {
                        (*core::ptr::addr_of_mut!(PCI_DEVICES))[count] = Some(PciDevice {
                            addr: faddr,
                            vendor_id: vid,
                            device_id: did,
                            class,
                            subclass,
                            prog_if,
                        });
                        *core::ptr::addr_of_mut!(PCI_COUNT) = count + 1;
                    }
                }
                count += 1;
            }
        }
    }

    serial::puts("PCI: found ");
    serial::dec(count as u64);
    serial::puts(" devices\n");
}

pub fn find_by_class(class: u8, subclass: u8, prog_if: u8) -> Option<PciAddr> {
    let cnt = unsafe { *core::ptr::addr_of!(PCI_COUNT) };
    for i in 0..cnt {
        if let Some(ref dev) = unsafe { &(*core::ptr::addr_of!(PCI_DEVICES))[i] } {
            if dev.class == class && dev.subclass == subclass && dev.prog_if == prog_if {
                return Some(dev.addr);
            }
        }
    }
    None
}

pub fn dump() {
    let cnt = unsafe { *core::ptr::addr_of!(PCI_COUNT) };
    for i in 0..cnt {
        if let Some(ref dev) = unsafe { &(*core::ptr::addr_of!(PCI_DEVICES))[i] } {
            serial::puts("  ");
            serial::hex(dev.addr.bus as u64);
            serial::puts(":");
            serial::hex(dev.addr.dev as u64);
            serial::puts(".");
            serial::hex(dev.addr.func as u64);
            serial::puts(" ");
            serial::hex(dev.vendor_id as u64);
            serial::puts(":");
            serial::hex(dev.device_id as u64);
            serial::puts(" class=");
            serial::hex(dev.class as u64);
            serial::puts(":");
            serial::hex(dev.subclass as u64);
            serial::puts(":");
            serial::hex(dev.prog_if as u64);
            serial::puts("\n");
        }
    }
}
