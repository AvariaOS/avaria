#![no_std]
#![no_main]
#![allow(dead_code)]

mod kern;

use core::panic::PanicInfo;
use core::ptr::addr_of_mut;
use kern::fs::initrd::TarFs;
use kern::fs::vfs::FileSystem;
use kern::gfx::{psf::PsfFont, vesa::Framebuffer};
use kern::serial;
use avaria_api::avariaApi;
use limine::BaseRevision;
use kern::arch::x86_64::{cpu, gdt, idt, lapic, pic, smp, sse, tsc, tss};
use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, ModuleRequest, MpRequest,
    StackSizeRequest,
};

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(3);

#[used]
#[unsafe(link_section = ".requests")]
static FB_REQ: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MODULE_REQ: ModuleRequest = ModuleRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static STACK_REQ: StackSizeRequest = StackSizeRequest::new().with_size(0x10000);

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQ: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MMAP_REQ: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static SMP_REQ: MpRequest = MpRequest::new();

static mut KERNEL_FB: Option<Framebuffer> = None;
static mut KERNEL_FONT: Option<PsfFont<'static>> = None;
static mut KERNEL_FS: Option<TarFs<'static>> = None;
static mut HHDM_OFFSET: u64 = 0;

fn fb() -> &'static Framebuffer {
    unsafe { (*addr_of_mut!(KERNEL_FB)).as_ref().unwrap() }
}

fn font() -> &'static PsfFont<'static> {
    unsafe { (*addr_of_mut!(KERNEL_FONT)).as_ref().unwrap() }
}

fn fs() -> &'static TarFs<'static> {
    unsafe { (*addr_of_mut!(KERNEL_FS)).as_ref().unwrap() }
}

fn hhdm() -> u64 {
    unsafe { *core::ptr::addr_of!(HHDM_OFFSET) }
}

unsafe extern "C" fn api_serial_puts(ptr: *const u8, len: usize) {
    let s = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len)) };
    serial::puts(s);
}

unsafe extern "C" fn api_fb_draw_str(
    x: usize,
    y: usize,
    ptr: *const u8,
    len: usize,
    fg: u32,
    bg: u32,
) {
    let s = unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr, len)) };
    if let (Some(f), Some(fnt)) = unsafe {
        (
            (*addr_of_mut!(KERNEL_FB)).as_ref(),
            (*addr_of_mut!(KERNEL_FONT)).as_ref(),
        )
    } {
        fnt.draw_str(f, x, y, s, fg, bg);
    }
}

unsafe extern "C" fn api_fs_read(
    path_ptr: *const u8,
    path_len: usize,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
) -> i32 {
    let path = unsafe {
        core::str::from_utf8_unchecked(core::slice::from_raw_parts(path_ptr, path_len))
    };
    if let Some(f) = unsafe { (*addr_of_mut!(KERNEL_FS)).as_ref() } {
        if let Some(data) = f.read(path) {
            unsafe {
                *out_ptr = data.as_ptr();
                *out_len = data.len();
            }
            return 0;
        }
    }
    -1
}

unsafe extern "C" fn api_tsc_read() -> u64 {
    tsc::rdtsc()
}

unsafe extern "C" fn api_tsc_freq_khz() -> u64 {
    tsc::freq_khz()
}

unsafe extern "C" fn api_kmalloc(size: usize) -> *mut u8 {
    kern::mem::kmalloc(size)
}

unsafe extern "C" fn api_kfree(ptr: *mut u8, size: usize) {
    kern::mem::kfree(ptr, size)
}

unsafe extern "C" fn api_preempt_disable() {
    kern::sched::preempt_disable();
}

unsafe extern "C" fn api_preempt_enable() {
    kern::sched::preempt_enable();
}

static AVARIA_API: avariaApi = avariaApi {
    serial_puts: api_serial_puts,
    fb_draw_str: api_fb_draw_str,
    fs_read: api_fs_read,
    tsc_read: api_tsc_read,
    tsc_freq_khz: api_tsc_freq_khz,
    kmalloc: api_kmalloc,
    kfree: api_kfree,
    preempt_disable: api_preempt_disable,
    preempt_enable: api_preempt_enable,
};

#[unsafe(no_mangle)]
unsafe extern "C" fn _start() -> ! {
    serial::init();
    serial::puts("avaria booting...\n");

    if !BASE_REVISION.is_supported() {
        serial::puts("FATAL: base revision not supported\n");
        hlt_loop();
    }

    gdt::init();
    serial::puts("GDT loaded\n");

    idt::init();
    serial::puts("IDT loaded\n");

    pic::remap(32, 40);
    pic::disable();
    serial::puts("PIC remapped & disabled\n");

    tss::init();
    gdt::install_tss(tss::tss_ptr(), tss::tss_size());
    tss::load(gdt::TSS_SEL);
    serial::puts("TSS loaded\n");

    kern::kxvm::syscall::init();

    sse::init();
    serial::puts("SSE/AVX ");
    if cpu::has_feature_sse() { serial::puts("SSE ") }
    if cpu::has_feature_sse2() { serial::puts("SSE2 ") }
    if cpu::has_feature_sse3() { serial::puts("SSE3 ") }
    if cpu::has_feature_sse41() { serial::puts("SSE4.1 ") }
    if cpu::has_feature_sse42() { serial::puts("SSE4.2 ") }
    if cpu::has_feature_avx() { serial::puts("AVX ") }
    serial::puts("\n");

    if cpu::has_tsc() {
        tsc::calibrate();
        serial::puts("TSC: ");
        serial::dec(tsc::freq_mhz());
        serial::puts(" MHz");
        if cpu::has_invariant_tsc() {
            serial::puts(" (invariant)");
        }
        serial::puts("\n");
    }

    let vendor = cpu::cpu_vendor();
    serial::puts("CPU: ");
    if let Ok(s) = core::str::from_utf8(&vendor) {
        serial::puts(s);
    }
    serial::puts("\n");

    let init_fb = match FB_REQ.get_response() {
        Some(resp) => match resp.framebuffers().next() {
            Some(f) => {
                serial::puts("FB ");
                serial::dec(f.width() as u64);
                serial::puts("x");
                serial::dec(f.height() as u64);
                serial::puts("\n");
                Framebuffer::new(
                    f.addr() as *mut u8,
                    f.width() as usize,
                    f.height() as usize,
                    f.pitch() as usize,
                    (f.bpp() as usize) / 8,
                )
            }
            None => {
                serial::puts("FATAL: no framebuffer\n");
                hlt_loop();
            }
        },
        None => {
            serial::puts("FATAL: framebuffer request failed\n");
            hlt_loop();
        }
    };

    let hhdm_offset = match HHDM_REQ.get_response() {
        Some(resp) => {
            serial::puts("HHDM offset=0x");
            serial::hex(resp.offset() as u64);
            serial::puts("\n");
            resp.offset() as u64
        }
        None => {
            serial::puts("FATAL: HHDM request failed\n");
            hlt_loop();
        }
    };
    unsafe { *addr_of_mut!(HHDM_OFFSET) = hhdm_offset };

    if let Some(mmap) = MMAP_REQ.get_response() {
        let entries: &[&limine::memory_map::Entry] = mmap.entries();
        kern::mem::init(hhdm_offset, entries);
        serial::puts("Memory: ");
        serial::dec((kern::mem::total_pages() * 4) as u64);
        serial::puts(" KB (");
        serial::dec(kern::mem::total_pages() as u64);
        serial::puts(" pages)\n");
    } else {
        serial::puts("FATAL: memory map request failed\n");
        hlt_loop();
    }

    if let Some(smp_resp) = SMP_REQ.get_response() {
        let cpus = smp_resp.cpus();
        let bsp = smp_resp.bsp_lapic_id();
        serial::puts("SMP: ");
        serial::dec(cpus.len() as u64);
        serial::puts(" CPUs, BSP LAPIC=");
        serial::dec(bsp as u64);
        serial::puts("\n");
        smp::init(bsp, cpus);
        serial::puts("SMP: ");
        serial::dec(smp::online_count() as u64);
        serial::puts(" CPUs online\n");
    }

    lapic::init(hhdm_offset);

    cpu::sti();

    kern::drv::ps2::init();

    kern::drv::pci::scan();
    kern::drv::pci::dump();
    kern::drv::disk::ahci::init();

    let ahci_ports = kern::drv::disk::ahci::active_ports();
    if ahci_ports != 0 {
        let port = ahci_ports.trailing_zeros() as usize;
        if let Some(ext3) = kern::fs::ext3::Ext3::mount(port) {
            ext3.dump_root();
        }
    }

    init_fb.clear(0x00, 0x00, 0x80);
    unsafe { *addr_of_mut!(KERNEL_FB) = Some(init_fb) };

    let initrd_data = match find_module(b"initrd") {
        Some(data) => data,
        None => {
            serial::puts("WARN: initrd not found\n");
            serial::puts("avaria ready.\n");
            hlt_loop();
        }
    };

    unsafe {
        *addr_of_mut!(KERNEL_FS) = match TarFs::parse(initrd_data) {
            Some(f) => {
                serial::puts("initrd: tarfs mounted\n");
                Some(f)
            }
            None => {
                serial::puts("FATAL: failed to parse initrd\n");
                hlt_loop();
            }
        };
    }

    if let Some(font_data) = fs().read("res/UniCyrX-ibm-8x16.psf") {
        serial::puts("Font loaded, size=");
        serial::dec(font_data.len() as u64);
        serial::puts("\n");

        if let Some(f) = PsfFont::parse(font_data) {
            f.draw_str(fb(), 16, 16, "РВИ ПИZДУ", 0xFFFFFF, 0x000080);
            unsafe { *addr_of_mut!(KERNEL_FONT) = Some(f) };
        } else {
            serial::puts("WARN: failed to parse PSF font\n");
        }
    } else {
        serial::puts("WARN: font not found in initrd\n");
    }

    kern::sched::init();

    const MAX_MODULES: usize = 16;
    static mut MODULE_BUFS: [[u8; 0x10000]; MAX_MODULES] = [[0; 0x10000]; MAX_MODULES];
    let mut mod_idx = 0usize;
    fs().for_each_matching("boot/modules/", ".ko", |path, ko_data| {
        if mod_idx >= MAX_MODULES {
            serial::puts("WARN: too many modules, skipping ");
            serial::puts(path);
            serial::puts("\n");
            return;
        }
        serial::puts("Loading ");
        serial::puts(path);
        serial::puts(", size=");
        serial::dec(ko_data.len() as u64);
        serial::puts("\n");

        let buf = unsafe { &mut (*addr_of_mut!(MODULE_BUFS))[mod_idx] };

        match avaria_elf::load_at(ko_data, buf) {
            Ok(loaded) => {
                unsafe {
                    kern::mem::make_executable(
                        hhdm_offset,
                        loaded.load_base as usize,
                        loaded.load_size,
                    );
                };
                let api_ptr = &AVARIA_API as *const avariaApi as *const ();
                kern::sched::spawn(loaded.entry, api_ptr, path.as_bytes());
                mod_idx += 1;
            }
            Err(_) => {
                serial::puts("WARN: failed to load ");
                serial::puts(path);
                serial::puts("\n");
            }
        }
    });

    kern::sched::spawn(shell_task_entry as u64, core::ptr::null(), b"shell");

    serial::puts("avaria ready.\n");

    kern::sched::start();
}

const SHELL_FG: u32 = 0xFFFFFF;
const SHELL_BG: u32 = 0x000080;
const SHELL_PROMPT: &str = "avaria> ";
const SHELL_CHAR_W: usize = 8;
const SHELL_CHAR_H: usize = 16;
const SHELL_PAD_X: usize = 8;
const SHELL_PAD_Y: usize = 8;

fn shell_draw_at(col: usize, row: usize, ch: u32) {
    if let Some(f) = unsafe { (*core::ptr::addr_of!(KERNEL_FONT)).as_ref() } {
        f.draw_char(
            fb(),
            SHELL_PAD_X + col * SHELL_CHAR_W,
            SHELL_PAD_Y + row * SHELL_CHAR_H,
            ch, SHELL_FG, SHELL_BG,
        );
    }
}

fn shell_draw_prompt(row: usize) {
    for (i, b) in SHELL_PROMPT.bytes().enumerate() {
        shell_draw_at(i, row, b as u32);
    }
}

unsafe extern "C" fn shell_task_entry(_api: *const ()) -> i32 {
    const LINE_MAX: usize = 256;

    let mut line_buf = [0u8; LINE_MAX];
    let mut line_len: usize = 0;

    let cols = (fb().width - SHELL_PAD_X * 2) / SHELL_CHAR_W;
    let rows = (fb().height - SHELL_PAD_Y * 2) / SHELL_CHAR_H;
    let mut cur_row: usize = 0;
    let prompt_len = SHELL_PROMPT.len();
    let mut cur_col = prompt_len;

    serial::puts("shell: started as task\n");

    fb().clear(0x00, 0x00, 0x80);
    shell_draw_prompt(cur_row);
    shell_draw_at(cur_col, cur_row, b'_' as u32);

    loop {
        if let Some(ch) = kern::drv::ps2::read_key() {
            match ch {
                b'\n' => {
                    shell_draw_at(cur_col, cur_row, b' ' as u32);

                    cur_row += 1;
                    if cur_row >= rows {
                        cur_row = 0;
                        fb().clear(0x00, 0x00, 0x80);
                    }

                    if line_len > 0 {
                        let prefix = b"> ";
                        for (i, &b) in prefix.iter().enumerate() {
                            shell_draw_at(i, cur_row, b as u32);
                        }
                        for i in 0..line_len {
                            shell_draw_at(prefix.len() + i, cur_row, line_buf[i] as u32);
                        }
                        cur_row += 1;
                        if cur_row >= rows {
                            cur_row = 0;
                            fb().clear(0x00, 0x00, 0x80);
                        }
                    }

                    line_len = 0;
                    cur_col = prompt_len;
                    shell_draw_prompt(cur_row);
                    shell_draw_at(cur_col, cur_row, b'_' as u32);
                }
                0x08 => {
                    if line_len > 0 {
                        shell_draw_at(cur_col, cur_row, b' ' as u32);
                        line_len -= 1;
                        cur_col -= 1;
                        shell_draw_at(cur_col, cur_row, b'_' as u32);
                    }
                }
                _ => {
                    if line_len < LINE_MAX - 1 && cur_col < cols - 1 {
                        shell_draw_at(cur_col, cur_row, ch as u32);
                        line_buf[line_len] = ch;
                        line_len += 1;
                        cur_col += 1;
                        shell_draw_at(cur_col, cur_row, b'_' as u32);
                    }
                }
            }
        } else {
            unsafe { core::arch::asm!("hlt", options(nostack, nomem)) };
        }
    }
}

fn find_module(name: &[u8]) -> Option<&'static [u8]> {
    let resp = MODULE_REQ.get_response()?;
    for m in resp.modules() {
        let cmdline = m.string().to_bytes();
        if cmdline == name {
            let addr = m.addr();
            let size = m.size() as usize;
            return Some(unsafe { core::slice::from_raw_parts(addr, size) });
        }
    }
    None
}

fn hlt_loop() -> ! {
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nostack, nomem));
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial::puts("\n!!! KERNEL PANIC !!!\n");
    if let Some(loc) = info.location() {
        serial::puts("  ");
        serial::puts(loc.file());
        serial::puts(":");
        serial::dec(loc.line() as u64);
        serial::puts("\n");
    }
    hlt_loop();
}
