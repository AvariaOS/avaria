
use core::arch::asm;
use super::cpu;

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const LAPIC_PHYS: u64 = 0xFEE0_0000;

const SVR: usize = 0xF0;
const LINT0: usize = 0x350;
const LINT1: usize = 0x360;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_PWT: u64 = 1 << 3;
const PTE_PCD: u64 = 1 << 4;
const PTE_NX: u64 = 1 << 63;

static mut LAPIC_VIRT: u64 = 0;

unsafe fn phys_to_virt(hhdm: u64, phys: u64) -> *mut u64 {
    (hhdm + phys) as *mut u64
}

unsafe fn map_lapic_page(hhdm: u64) -> u64 {
    let cr3: u64;
    asm!("mov {}, cr3", out(reg) cr3, options(nostack, nomem));
    let pml4_phys = cr3 & !0xFFF;

    let vaddr = hhdm + LAPIC_PHYS;
    let pml4_idx = ((vaddr as usize) >> 39) & 0x1FF;
    let pdpt_idx = ((vaddr as usize) >> 30) & 0x1FF;
    let pd_idx = ((vaddr as usize) >> 21) & 0x1FF;
    let pt_idx = ((vaddr as usize) >> 12) & 0x1FF;

    let pml4 = phys_to_virt(hhdm, pml4_phys);
    let pml4e = *pml4.add(pml4_idx);
    if pml4e & PTE_PRESENT == 0 {
        crate::kern::serial::puts("lapic: PML4 not present for LAPIC mapping\n");
        return 0;
    }

    let pdpt_phys = pml4e & 0x000F_FFFF_FFFF_F000;
    let pdpt = phys_to_virt(hhdm, pdpt_phys);
    let pdpte = *pdpt.add(pdpt_idx);

    if pdpte & PTE_PRESENT != 0 && pdpte & (1 << 7) != 0 {
        return vaddr;
    }
    if pdpte & PTE_PRESENT == 0 {
        crate::kern::serial::puts("lapic: PDPT not present\n");
        return 0;
    }

    let pd_phys = pdpte & 0x000F_FFFF_FFFF_F000;
    let pd = phys_to_virt(hhdm, pd_phys);
    let pde = *pd.add(pd_idx);

    if pde & PTE_PRESENT != 0 && pde & (1 << 7) != 0 {
        return vaddr;
    }

    if pde & PTE_PRESENT == 0 {
        let pt_page = crate::kern::mem::alloc_pages(0);
        match pt_page {
            Some(va) => {
                core::ptr::write_bytes(va as *mut u8, 0, 4096);
                let pt_phys_new = crate::kern::mem::virt_to_phys(va);
                *pd.add(pd_idx) = pt_phys_new | PTE_PRESENT | PTE_WRITABLE;
                let pt = phys_to_virt(hhdm, pt_phys_new);
                *pt.add(pt_idx) = LAPIC_PHYS | PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD;
            }
            None => {
                crate::kern::serial::puts("lapic: cannot alloc page table\n");
                return 0;
            }
        }
    } else {
        let pt_phys = pde & 0x000F_FFFF_FFFF_F000;
        let pt = phys_to_virt(hhdm, pt_phys);
        let pte = *pt.add(pt_idx);
        if pte & PTE_PRESENT == 0 {
            *pt.add(pt_idx) = LAPIC_PHYS | PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD;
        }
    }

    asm!("invlpg [{}]", in(reg) vaddr, options(nostack));

    vaddr
}

fn read_reg(off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((LAPIC_VIRT + off as u64) as *const u32) }
}

fn write_reg(off: usize, val: u32) {
    unsafe { core::ptr::write_volatile((LAPIC_VIRT + off as u64) as *mut u32, val) }
}

pub fn init(hhdm: u64) {
    use crate::kern::serial;

    let apic_msr = cpu::rdmsr(IA32_APIC_BASE_MSR);
    serial::puts("lapic: MSR=0x");
    serial::hex(apic_msr);
    serial::puts("\n");

    let vaddr = unsafe { map_lapic_page(hhdm) };
    if vaddr == 0 {
        serial::puts("lapic: mapping failed\n");
        return;
    }
    unsafe { LAPIC_VIRT = vaddr };
    serial::puts("lapic: mapped at 0x");
    serial::hex(vaddr);
    serial::puts("\n");

    let svr_before = read_reg(SVR);
    let lint0_before = read_reg(LINT0);
    serial::puts("lapic: SVR=0x");
    serial::hex(svr_before as u64);
    serial::puts(" LINT0=0x");
    serial::hex(lint0_before as u64);
    serial::puts("\n");

    write_reg(SVR, svr_before | (1 << 8) | 0xFF);

    write_reg(LINT0, 0b111 << 8);

    write_reg(LINT1, 0b100 << 8);

    let svr_after = read_reg(SVR);
    let lint0_after = read_reg(LINT0);
    serial::puts("lapic: SVR=0x");
    serial::hex(svr_after as u64);
    serial::puts(" LINT0=0x");
    serial::hex(lint0_after as u64);
    serial::puts(" OK\n");
}
