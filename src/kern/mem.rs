use core::ptr::addr_of_mut;
use limine::memory_map::EntryType;

use super::mm::buddy::BuddyAllocator;
use super::mm::slab::SlabAllocator;

const PAGE_SIZE: usize = 0x1000;
const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_PWT: u64 = 1 << 3;
const PTE_PCD: u64 = 1 << 4;
const PTE_NX: u64 = 1 << 63;

static mut BUDDY: BuddyAllocator = BuddyAllocator::new();
static mut SLAB: SlabAllocator = SlabAllocator::new();
static mut HHDM: u64 = 0;

pub fn init(hhdm_offset: u64, entries: &[&limine::memory_map::Entry]) {
    unsafe { *addr_of_mut!(HHDM) = hhdm_offset };

    let mut best_base = 0u64;
    let mut best_size = 0u64;

    for entry in entries {
        if entry.entry_type == EntryType::USABLE && entry.length > best_size {
            best_base = entry.base;
            best_size = entry.length;
        }
    }

    if best_size == 0 {
        return;
    }

    let virt_base = hhdm_offset + best_base;
    let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
    buddy.init(virt_base as usize, best_size as usize);
}

pub fn kmalloc(size: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }

    if size <= 2048 {
        let slab = unsafe { &mut *addr_of_mut!(SLAB) };
        let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
        if let Some(ptr) = slab.alloc(size, buddy) {
            return ptr;
        }
    }

    let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
    let mut order = 0;
    while (1 << order) < pages {
        order += 1;
    }
    let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
    match buddy.alloc_pages(order) {
        Some(addr) => addr as *mut u8,
        None => core::ptr::null_mut(),
    }
}

pub fn kfree(ptr: *mut u8, size: usize) {
    if ptr.is_null() || size == 0 {
        return;
    }

    if size <= 2048 {
        let slab = unsafe { &mut *addr_of_mut!(SLAB) };
        let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
        if slab.free(ptr, buddy) {
            return;
        }
    }

    let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
    let mut order = 0;
    while (1 << order) < pages {
        order += 1;
    }
    let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
    buddy.free_pages(ptr as usize, order);
}

pub fn alloc_pages(order: usize) -> Option<usize> {
    let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
    buddy.alloc_pages(order)
}

pub fn free_pages(addr: usize, order: usize) {
    let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
    buddy.free_pages(addr, order);
}

pub fn total_pages() -> usize {
    let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
    buddy.total_pages()
}

pub fn hhdm_offset() -> u64 {
    unsafe { *core::ptr::addr_of!(HHDM) }
}

pub fn virt_to_phys(vaddr: usize) -> u64 {
    let hhdm = hhdm_offset();
    (vaddr as u64).wrapping_sub(hhdm)
}

pub fn phys_to_virt_pub(phys: u64) -> usize {
    let hhdm = hhdm_offset();
    (hhdm + phys) as usize
}

pub fn kmalloc_aligned(size: usize, align: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let alloc_size = if size < align { align } else { size };
    let pages = (alloc_size + PAGE_SIZE - 1) / PAGE_SIZE;
    let mut order = 0;
    while (1 << order) < pages {
        order += 1;
    }
    let buddy = unsafe { &mut *addr_of_mut!(BUDDY) };
    match buddy.alloc_pages(order) {
        Some(addr) => addr as *mut u8,
        None => core::ptr::null_mut(),
    }
}

unsafe fn phys_to_virt(hhdm_offset: u64, phys: u64) -> *mut u64 {
    (hhdm_offset + phys) as *mut u64
}

pub unsafe fn make_executable(hhdm_offset: u64, vaddr: usize, size: usize) {
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nostack, nomem));
    }
    let pml4_phys = cr3 & !0xFFF;

    let mut addr = vaddr & !0xFFF;
    let end = (vaddr + size + 0xFFF) & !0xFFF;

    while addr < end {
        let pml4_idx = (addr >> 39) & 0x1FF;
        let pdpt_idx = (addr >> 30) & 0x1FF;
        let pd_idx = (addr >> 21) & 0x1FF;
        let pt_idx = (addr >> 12) & 0x1FF;

        let pml4 = unsafe { phys_to_virt(hhdm_offset, pml4_phys) };
        let pml4e = unsafe { *pml4.add(pml4_idx) };
        if pml4e & PTE_PRESENT == 0 {
            addr += PAGE_SIZE;
            continue;
        }

        let pdpt_phys = pml4e & 0x000F_FFFF_FFFF_F000;
        let pdpt = unsafe { phys_to_virt(hhdm_offset, pdpt_phys) };
        let pdpte = unsafe { *pdpt.add(pdpt_idx) };
        if pdpte & PTE_PRESENT == 0 {
            addr += PAGE_SIZE;
            continue;
        }
        if pdpte & (1 << 7) != 0 {
            unsafe { *pdpt.add(pdpt_idx) = pdpte & !PTE_NX | PTE_WRITABLE };
            addr += PAGE_SIZE;
            continue;
        }

        let pd_phys = pdpte & 0x000F_FFFF_FFFF_F000;
        let pd = unsafe { phys_to_virt(hhdm_offset, pd_phys) };
        let pde = unsafe { *pd.add(pd_idx) };
        if pde & PTE_PRESENT == 0 {
            addr += PAGE_SIZE;
            continue;
        }
        if pde & (1 << 7) != 0 {
            unsafe { *pd.add(pd_idx) = pde & !PTE_NX | PTE_WRITABLE };
            addr += PAGE_SIZE;
            continue;
        }

        let pt_phys = pde & 0x000F_FFFF_FFFF_F000;
        let pt = unsafe { phys_to_virt(hhdm_offset, pt_phys) };
        let pte = unsafe { *pt.add(pt_idx) };
        if pte & PTE_PRESENT != 0 {
            unsafe { *pt.add(pt_idx) = pte & !PTE_NX | PTE_WRITABLE };
        }

        addr += PAGE_SIZE;
    }

    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack));
    }
}

pub unsafe fn map_mmio(phys_base: u64, num_pages: usize) -> usize {
    let hhdm = hhdm_offset();
    let cr3: u64;
    core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nostack, nomem));
    let pml4_phys = cr3 & !0xFFF;

    for page_i in 0..num_pages {
        let phys = phys_base + (page_i as u64) * 0x1000;
        let vaddr = (hhdm + phys) as usize;

        let pml4_idx = (vaddr >> 39) & 0x1FF;
        let pdpt_idx = (vaddr >> 30) & 0x1FF;
        let pd_idx = (vaddr >> 21) & 0x1FF;
        let pt_idx = (vaddr >> 12) & 0x1FF;

        let pml4 = phys_to_virt(hhdm, pml4_phys);
        let pml4e = *pml4.add(pml4_idx);
        if pml4e & PTE_PRESENT == 0 {
            let page = match alloc_pages(0) {
                Some(va) => va,
                None => return 0,
            };
            core::ptr::write_bytes(page as *mut u8, 0, 4096);
            let page_phys = virt_to_phys(page);
            *pml4.add(pml4_idx) = page_phys | PTE_PRESENT | PTE_WRITABLE;
        }

        let pml4e = *pml4.add(pml4_idx);
        let pdpt_phys = pml4e & 0x000F_FFFF_FFFF_F000;
        let pdpt = phys_to_virt(hhdm, pdpt_phys);
        let pdpte = *pdpt.add(pdpt_idx);

        if pdpte & PTE_PRESENT != 0 && pdpte & (1 << 7) != 0 {
            core::arch::asm!("invlpg [{}]", in(reg) vaddr, options(nostack));
            continue;
        }

        if pdpte & PTE_PRESENT == 0 {
            let page = match alloc_pages(0) {
                Some(va) => va,
                None => return 0,
            };
            core::ptr::write_bytes(page as *mut u8, 0, 4096);
            let page_phys = virt_to_phys(page);
            *pdpt.add(pdpt_idx) = page_phys | PTE_PRESENT | PTE_WRITABLE;
        }

        let pdpte = *pdpt.add(pdpt_idx);
        let pd_phys = pdpte & 0x000F_FFFF_FFFF_F000;
        let pd = phys_to_virt(hhdm, pd_phys);
        let pde = *pd.add(pd_idx);

        if pde & PTE_PRESENT != 0 && pde & (1 << 7) != 0 {
            core::arch::asm!("invlpg [{}]", in(reg) vaddr, options(nostack));
            continue;
        }

        if pde & PTE_PRESENT == 0 {
            let page = match alloc_pages(0) {
                Some(va) => va,
                None => return 0,
            };
            core::ptr::write_bytes(page as *mut u8, 0, 4096);
            let page_phys = virt_to_phys(page);
            *pd.add(pd_idx) = page_phys | PTE_PRESENT | PTE_WRITABLE;
        }

        let pde = *pd.add(pd_idx);
        let pt_phys = pde & 0x000F_FFFF_FFFF_F000;
        let pt = phys_to_virt(hhdm, pt_phys);
        let pte = *pt.add(pt_idx);

        if pte & PTE_PRESENT == 0 {
            *pt.add(pt_idx) = phys | PTE_PRESENT | PTE_WRITABLE | PTE_PWT | PTE_PCD | PTE_NX;
        }

        core::arch::asm!("invlpg [{}]", in(reg) vaddr, options(nostack));
    }

    (hhdm + phys_base) as usize
}
