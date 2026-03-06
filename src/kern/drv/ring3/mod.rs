use core::arch::asm;
use crate::kern::arch::x86_64::gdt;
use crate::kern::mem;

const USER_STACK_SIZE: usize = 0x4000;
const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_NX: u64 = 1 << 63;

pub unsafe fn make_user_accessible(hhdm_offset: u64, vaddr: usize, size: usize) {
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nostack, nomem));
    }
    let pml4_phys = cr3 & !0xFFF;

    let mut addr = vaddr & !0xFFF;
    let end = (vaddr + size + 0xFFF) & !0xFFF;

    while addr < end {
        let pml4_idx = (addr >> 39) & 0x1FF;
        let pdpt_idx = (addr >> 30) & 0x1FF;
        let pd_idx = (addr >> 21) & 0x1FF;
        let pt_idx = (addr >> 12) & 0x1FF;

        let pml4 = phys_to_virt(hhdm_offset, pml4_phys);
        let pml4e = *pml4.add(pml4_idx);
        if pml4e & PTE_PRESENT == 0 {
            addr += 0x1000;
            continue;
        }
        *pml4.add(pml4_idx) = pml4e | PTE_USER;

        let pdpt_phys = pml4e & 0x000F_FFFF_FFFF_F000;
        let pdpt = phys_to_virt(hhdm_offset, pdpt_phys);
        let pdpte = *pdpt.add(pdpt_idx);
        if pdpte & PTE_PRESENT == 0 {
            addr += 0x1000;
            continue;
        }
        *pdpt.add(pdpt_idx) = pdpte | PTE_USER;

        if pdpte & (1 << 7) != 0 {
            *pdpt.add(pdpt_idx) = (pdpte | PTE_USER | PTE_WRITABLE) & !PTE_NX;
            addr += 0x1000;
            continue;
        }

        let pd_phys = pdpte & 0x000F_FFFF_FFFF_F000;
        let pd = phys_to_virt(hhdm_offset, pd_phys);
        let pde = *pd.add(pd_idx);
        if pde & PTE_PRESENT == 0 {
            addr += 0x1000;
            continue;
        }
        *pd.add(pd_idx) = pde | PTE_USER;

        if pde & (1 << 7) != 0 {
            *pd.add(pd_idx) = (pde | PTE_USER | PTE_WRITABLE) & !PTE_NX;
            addr += 0x1000;
            continue;
        }

        let pt_phys = pde & 0x000F_FFFF_FFFF_F000;
        let pt = phys_to_virt(hhdm_offset, pt_phys);
        let pte = *pt.add(pt_idx);
        if pte & PTE_PRESENT != 0 {
            *pt.add(pt_idx) = (pte | PTE_USER | PTE_WRITABLE) & !PTE_NX;
        }

        addr += 0x1000;
    }

    asm!("mov cr3, {}", in(reg) cr3, options(nostack));
}

#[inline(always)]
unsafe fn phys_to_virt(hhdm_offset: u64, phys: u64) -> *mut u64 {
    (hhdm_offset + phys) as *mut u64
}

pub unsafe fn enter_ring3(entry: u64, hhdm_offset: u64) {
    let user_stack_ptr = mem::kmalloc(USER_STACK_SIZE);
    if user_stack_ptr.is_null() {
        crate::kern::serial::puts("ring3: failed to allocate user stack\n");
        return;
    }
    let user_stack_top = user_stack_ptr as u64 + USER_STACK_SIZE as u64;

    make_user_accessible(hhdm_offset, user_stack_ptr as usize, USER_STACK_SIZE);

    let user_cs = gdt::USER_CS as u64;
    let user_ds = gdt::USER_DS as u64;

    asm!(
        "mov ds, {ds:x}",
        "mov es, {ds:x}",
        "mov fs, {ds:x}",
        "mov gs, {ds:x}",
        "push {ss}",
        "push {rsp_u}",
        "push 0x202",
        "push {cs}",
        "push {rip}",
        "iretq",
        ds = in(reg) user_ds,
        ss = in(reg) user_ds,
        rsp_u = in(reg) user_stack_top,
        cs = in(reg) user_cs,
        rip = in(reg) entry,
        options(nostack, noreturn),
    );
}
