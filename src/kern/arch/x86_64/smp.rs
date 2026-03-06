use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use limine::mp::Cpu;

const MAX_CPUS: usize = 64;

static CPU_COUNT: AtomicU32 = AtomicU32::new(1);
static BSP_LAPIC_ID: AtomicU32 = AtomicU32::new(0);
static APS_READY: AtomicU32 = AtomicU32::new(0);
static ALL_GO: AtomicBool = AtomicBool::new(false);

static mut CPU_ONLINE: [AtomicBool; MAX_CPUS] = {
    const INIT: AtomicBool = AtomicBool::new(false);
    [INIT; MAX_CPUS]
};

pub fn init(bsp_id: u32, cpus: &[&Cpu]) {
    BSP_LAPIC_ID.store(bsp_id, Ordering::Relaxed);
    CPU_COUNT.store(cpus.len() as u32, Ordering::Relaxed);

    let bsp_slot = (bsp_id as usize) % MAX_CPUS;
    unsafe { (*core::ptr::addr_of!(CPU_ONLINE))[bsp_slot].store(true, Ordering::Relaxed) };

    for cpu in cpus {
        if cpu.lapic_id == bsp_id {
            continue;
        }
        cpu.goto_address.write(ap_entry);
    }

    while APS_READY.load(Ordering::Acquire) < (cpus.len() as u32 - 1) {
        core::hint::spin_loop();
    }

    ALL_GO.store(true, Ordering::Release);
}

unsafe extern "C" fn ap_entry(cpu: &Cpu) -> ! {
    super::sse::init();

    let slot = (cpu.lapic_id as usize) % MAX_CPUS;
    unsafe { (*core::ptr::addr_of!(CPU_ONLINE))[slot].store(true, Ordering::Release) };

    APS_READY.fetch_add(1, Ordering::Release);

    while !ALL_GO.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }

    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, nomem)) };
    }
}

pub fn cpu_count() -> u32 {
    CPU_COUNT.load(Ordering::Relaxed)
}

pub fn bsp_id() -> u32 {
    BSP_LAPIC_ID.load(Ordering::Relaxed)
}

pub fn is_online(lapic_id: u32) -> bool {
    let slot = (lapic_id as usize) % MAX_CPUS;
    unsafe { (*core::ptr::addr_of!(CPU_ONLINE))[slot].load(Ordering::Relaxed) }
}

pub fn online_count() -> u32 {
    let mut count = 0;
    for i in 0..MAX_CPUS {
        if unsafe { (*core::ptr::addr_of!(CPU_ONLINE))[i].load(Ordering::Relaxed) } {
            count += 1;
        }
    }
    count
}
