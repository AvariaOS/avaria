#![no_std]
#![no_main]

use avaria_api::avariaApi;

const ITERS: usize = 1_000_000;

fn print_num(api: &avariaApi, mut v: u64) {
    if v == 0 {
        api.serial_print("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        let ch = [buf[i]];
        let s = unsafe { core::str::from_utf8_unchecked(&ch) };
        api.serial_print(s);
    }
}

fn bench_slab(api: &avariaApi, size: usize) {
    api.serial_print("  slab ");
    print_num(api, size as u64);
    api.serial_print("B x");
    print_num(api, ITERS as u64);
    api.serial_print(": ");

    api.disable_preempt();
    let start = api.rdtsc();
    for _ in 0..ITERS {
        let ptr = api.alloc(size);
        if !ptr.is_null() {
            unsafe { core::ptr::write_volatile(ptr, 0xAA) };
            api.free(ptr, size);
        }
    }
    let end = api.rdtsc();
    api.enable_preempt();
    let us = api.ticks_to_us(end - start);
    let ms = us / 1000;
    let us_frac = us % 1000;

    print_num(api, ms);
    api.serial_print(".");
    if us_frac < 100 { api.serial_print("0") }
    if us_frac < 10 { api.serial_print("0") }
    print_num(api, us_frac);
    api.serial_print(" ms\n");
}

fn bench_buddy(api: &avariaApi, order: usize) {
    let pages = 1usize << order;
    let size = pages * 4096;

    api.serial_print("  buddy order=");
    print_num(api, order as u64);
    api.serial_print(" (");
    print_num(api, (size / 1024) as u64);
    api.serial_print("K) x");
    print_num(api, ITERS as u64);
    api.serial_print(": ");

    api.disable_preempt();
    let start = api.rdtsc();
    for _ in 0..ITERS {
        let ptr = api.alloc(size);
        if !ptr.is_null() {
            unsafe { core::ptr::write_volatile(ptr, 0xBB) };
            api.free(ptr, size);
        }
    }
    let end = api.rdtsc();
    api.enable_preempt();
    let us = api.ticks_to_us(end - start);
    let ms = us / 1000;
    let us_frac = us % 1000;

    print_num(api, ms);
    api.serial_print(".");
    if us_frac < 100 { api.serial_print("0") }
    if us_frac < 10 { api.serial_print("0") }
    print_num(api, us_frac);
    api.serial_print(" ms\n");
}

fn bench_mixed(api: &avariaApi) {
    api.serial_print("  mixed alloc/free x");
    print_num(api, ITERS as u64);
    api.serial_print(": ");

    api.disable_preempt();
    let start = api.rdtsc();
    for i in 0..ITERS {
        let size = 16 + (i % 8) * 32;
        let ptr = api.alloc(size);
        if !ptr.is_null() {
            unsafe { core::ptr::write_volatile(ptr, 0xCC) };
            api.free(ptr, size);
        }
    }
    let end = api.rdtsc();
    api.enable_preempt();
    let us = api.ticks_to_us(end - start);
    let ms = us / 1000;
    let us_frac = us % 1000;

    print_num(api, ms);
    api.serial_print(".");
    if us_frac < 100 { api.serial_print("0") }
    if us_frac < 10 { api.serial_print("0") }
    print_num(api, us_frac);
    api.serial_print(" ms\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn _module_entry(api: &avariaApi) -> i32 {
    api.serial_print("\nMemtest: alloc benchmark\n");
    api.serial_print("TSC freq: ");
    print_num(api, api.tsc_khz() / 1000);
    api.serial_print(" MHz\n\n");

    api.serial_print("[slab allocator]\n");
    bench_slab(api, 16);
    bench_slab(api, 32);
    bench_slab(api, 64);
    bench_slab(api, 128);
    bench_slab(api, 256);
    bench_slab(api, 512);
    bench_slab(api, 1024);
    bench_slab(api, 2048);

    api.serial_print("\n[buddy allocator]\n");
    bench_buddy(api, 0);
    bench_buddy(api, 1);
    bench_buddy(api, 2);
    bench_buddy(api, 4);

    api.serial_print("\n[mixed workload]\n");
    bench_mixed(api);

    api.serial_print("\ndone.\n");
    0
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("hlt", options(nostack, nomem)) };
    }
}
