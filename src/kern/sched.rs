
use core::sync::atomic::{AtomicBool, Ordering};
use crate::kern::arch::x86_64::{cpu, idt::IsrContext, pic, pit, tss};
use crate::kern::serial;
use crate::kern::mem;

const MAX_TASKS: usize = 16;
const TASK_STACK_SIZE: usize = 0x4000;
const TASK_STACK_ORDER: usize = 2;
const TIMER_HZ: u32 = 100;
const TIMER_VECTOR: usize = 32;

#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
enum TaskState {
    Empty = 0,
    Ready = 1,
    Running = 2,
    Exited = 3,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Task {
    rip: u64,
    rsp: u64,
    rflags: u64,
    rax: u64, rbx: u64, rcx: u64, rdx: u64,
    rsi: u64, rdi: u64, rbp: u64,
    r8: u64, r9: u64, r10: u64, r11: u64,
    r12: u64, r13: u64, r14: u64, r15: u64,

    state: TaskState,
    id: u16,
    stack_base: usize,
    stack_size: usize,
    name: [u8; 16],
}

impl Task {
    const fn empty() -> Self {
        Self {
            rip: 0, rsp: 0, rflags: 0,
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            state: TaskState::Empty,
            id: 0,
            stack_base: 0,
            stack_size: 0,
            name: [0; 16],
        }
    }
}

static mut TASKS: [Task; MAX_TASKS] = [Task::empty(); MAX_TASKS];
static mut CURRENT: usize = 0;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static mut TICK_COUNT: u64 = 0;
static mut PREEMPT_COUNT: u32 = 0;

fn save_ctx(task: &mut Task, ctx: &IsrContext) {
    task.rip = ctx.rip;
    task.rsp = ctx.rsp;
    task.rflags = ctx.rflags;
    task.rax = ctx.rax;
    task.rbx = ctx.rbx;
    task.rcx = ctx.rcx;
    task.rdx = ctx.rdx;
    task.rsi = ctx.rsi;
    task.rdi = ctx.rdi;
    task.rbp = ctx.rbp;
    task.r8 = ctx.r8;
    task.r9 = ctx.r9;
    task.r10 = ctx.r10;
    task.r11 = ctx.r11;
    task.r12 = ctx.r12;
    task.r13 = ctx.r13;
    task.r14 = ctx.r14;
    task.r15 = ctx.r15;
}

fn load_ctx(task: &Task, ctx: &mut IsrContext) {
    ctx.rip = task.rip;
    ctx.rsp = task.rsp;
    ctx.rflags = task.rflags;
    ctx.rax = task.rax;
    ctx.rbx = task.rbx;
    ctx.rcx = task.rcx;
    ctx.rdx = task.rdx;
    ctx.rsi = task.rsi;
    ctx.rdi = task.rdi;
    ctx.rbp = task.rbp;
    ctx.r8 = task.r8;
    ctx.r9 = task.r9;
    ctx.r10 = task.r10;
    ctx.r11 = task.r11;
    ctx.r12 = task.r12;
    ctx.r13 = task.r13;
    ctx.r14 = task.r14;
    ctx.r15 = task.r15;
    ctx.cs = 0x08;
    ctx.ss = 0x10;
}

unsafe extern "C" fn task_trampoline(entry: u64, api: u64) {
    let entry_fn: extern "C" fn(*const ()) -> i32 = core::mem::transmute(entry);
    let ret = entry_fn(api as *const ());

    serial::puts("sched: task exited, ret=");
    serial::dec(ret as u64);
    serial::puts("\n");

    task_exit();
}

fn task_exit() -> ! {
    cpu::cli();
    let cur = unsafe { CURRENT };
    unsafe { TASKS[cur].state = TaskState::Exited };
    serial::puts("sched: task ");
    serial::dec(cur as u64);
    serial::puts(" exited\n");
    cpu::sti();
    loop { cpu::hlt(); }
}

fn timer_tick(ctx: &mut IsrContext) {
    unsafe { TICK_COUNT += 1 };

    if !ACTIVE.load(Ordering::Acquire) || unsafe { PREEMPT_COUNT } > 0 {
        pic::eoi(0);
        return;
    }

    let cur = unsafe { CURRENT };

    unsafe {
        save_ctx(&mut TASKS[cur], ctx);
        if TASKS[cur].state == TaskState::Running {
            TASKS[cur].state = TaskState::Ready;
        }
    }

    let mut next = cur;
    for _ in 0..MAX_TASKS {
        next = (next + 1) % MAX_TASKS;
        if unsafe { TASKS[next].state } == TaskState::Ready {
            break;
        }
    }

    if unsafe { TASKS[next].state } != TaskState::Ready {
        next = 0;
        if unsafe { TASKS[0].state } == TaskState::Exited {
            pic::eoi(0);
            return;
        }
    }

    unsafe {
        TASKS[next].state = TaskState::Running;
        CURRENT = next;
        load_ctx(&TASKS[next], ctx);

        tss::set_kernel_stack((TASKS[next].stack_base + TASKS[next].stack_size) as u64);
    }

    pic::eoi(0);
}

pub fn init() {
    serial::puts("sched: init\n");

    unsafe {
        TASKS[0].state = TaskState::Running;
        TASKS[0].id = 0;
        TASKS[0].name[..4].copy_from_slice(b"idle");
    }

    crate::kern::arch::x86_64::idt::register_ctx_handler(TIMER_VECTOR, timer_tick);

    pit::init(TIMER_HZ);
    pic::clear_mask(0);

    serial::puts("sched: PIT ");
    serial::dec(TIMER_HZ as u64);
    serial::puts(" Hz, IRQ0 enabled\n");
}

pub fn spawn(entry: u64, api: *const (), name: &[u8]) -> Option<u16> {
    cpu::cli();

    let mut slot = None;
    for i in 1..MAX_TASKS {
        if unsafe { TASKS[i].state } == TaskState::Empty
            || unsafe { TASKS[i].state } == TaskState::Exited
        {
            slot = Some(i);
            break;
        }
    }

    let i = match slot {
        Some(i) => i,
        None => {
            cpu::sti();
            serial::puts("sched: no free task slots\n");
            return None;
        }
    };

    let stack_base = match mem::alloc_pages(TASK_STACK_ORDER) {
        Some(addr) => addr,
        None => {
            cpu::sti();
            serial::puts("sched: cannot alloc stack\n");
            return None;
        }
    };

    let task = unsafe { &mut TASKS[i] };
    *task = Task::empty();
    task.id = i as u16;
    task.state = TaskState::Ready;
    task.stack_base = stack_base;
    task.stack_size = TASK_STACK_SIZE;

    let copy_len = name.len().min(15);
    task.name[..copy_len].copy_from_slice(&name[..copy_len]);

    task.rip = task_trampoline as u64;
    task.rsp = (stack_base + TASK_STACK_SIZE - 8) as u64;
    task.rflags = 0x202;
    task.rdi = entry;
    task.rsi = api as u64;

    serial::puts("sched: spawned task ");
    serial::dec(i as u64);
    serial::puts(" '");
    for j in 0..copy_len {
        serial::putb(name[j]);
    }
    serial::puts("'\n");

    cpu::sti();
    Some(i as u16)
}

pub fn start() -> ! {
    serial::puts("sched: starting\n");
    ACTIVE.store(true, Ordering::Release);

    loop {
        cpu::hlt();
    }
}

pub fn ticks() -> u64 {
    unsafe { TICK_COUNT }
}

pub fn preempt_disable() {
    unsafe { PREEMPT_COUNT += 1 };
}

pub fn preempt_enable() {
    unsafe {
        if PREEMPT_COUNT > 0 {
            PREEMPT_COUNT -= 1;
        }
    }
}
