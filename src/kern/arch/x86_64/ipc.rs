use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

const MAX_CHANNELS: usize = 32;
const RING_SIZE: usize = 64;
const MSG_SIZE: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Message {
    pub src: u32,
    pub msg_type: u32,
    pub data: [u8; MSG_SIZE - 8],
}

impl Message {
    pub const fn empty() -> Self {
        Self {
            src: 0,
            msg_type: 0,
            data: [0; MSG_SIZE - 8],
        }
    }

    pub fn new(src: u32, msg_type: u32) -> Self {
        Self {
            src,
            msg_type,
            data: [0; MSG_SIZE - 8],
        }
    }

    pub fn with_u64(mut self, val: u64) -> Self {
        self.data[..8].copy_from_slice(&val.to_le_bytes());
        self
    }

    pub fn read_u64(&self) -> u64 {
        u64::from_le_bytes(self.data[..8].try_into().unwrap_or([0; 8]))
    }

    pub fn with_bytes(mut self, bytes: &[u8]) -> Self {
        let len = bytes.len().min(self.data.len());
        self.data[..len].copy_from_slice(&bytes[..len]);
        self
    }
}

struct RingBuffer {
    buf: [Message; RING_SIZE],
    head: AtomicU32,
    tail: AtomicU32,
}

impl RingBuffer {
    const fn new() -> Self {
        Self {
            buf: [Message::empty(); RING_SIZE],
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
        }
    }

    fn push(&mut self, msg: Message) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let next = (head + 1) % RING_SIZE as u32;
        if next == tail {
            return false;
        }
        self.buf[head as usize] = msg;
        self.head.store(next, Ordering::Release);
        true
    }

    fn pop(&mut self) -> Option<Message> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let msg = self.buf[tail as usize];
        self.tail.store((tail + 1) % RING_SIZE as u32, Ordering::Release);
        Some(msg)
    }

    fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Relaxed) == self.head.load(Ordering::Relaxed)
    }

    fn len(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed) as usize;
        let tail = self.tail.load(Ordering::Relaxed) as usize;
        (head + RING_SIZE - tail) % RING_SIZE
    }
}

struct Channel {
    active: bool,
    ring: RingBuffer,
    owner: u32,
}

impl Channel {
    const fn new() -> Self {
        Self {
            active: false,
            ring: RingBuffer::new(),
            owner: 0,
        }
    }
}

static mut CHANNELS: [Channel; MAX_CHANNELS] = [const { Channel::new() }; MAX_CHANNELS];
static NEXT_CHANNEL: AtomicU32 = AtomicU32::new(0);

pub fn create_channel(owner: u32) -> Option<u32> {
    let id = NEXT_CHANNEL.fetch_add(1, Ordering::Relaxed);
    if id as usize >= MAX_CHANNELS {
        NEXT_CHANNEL.fetch_sub(1, Ordering::Relaxed);
        return None;
    }
    let ch = unsafe { &mut (*core::ptr::addr_of_mut!(CHANNELS))[id as usize] };
    ch.active = true;
    ch.owner = owner;
    Some(id)
}

pub fn send(channel_id: u32, msg: Message) -> bool {
    let idx = channel_id as usize;
    if idx >= MAX_CHANNELS {
        return false;
    }
    let ch = unsafe { &mut (*core::ptr::addr_of_mut!(CHANNELS))[idx] };
    if !ch.active {
        return false;
    }
    ch.ring.push(msg)
}

pub fn recv(channel_id: u32) -> Option<Message> {
    let idx = channel_id as usize;
    if idx >= MAX_CHANNELS {
        return None;
    }
    let ch = unsafe { &mut (*core::ptr::addr_of_mut!(CHANNELS))[idx] };
    if !ch.active {
        return None;
    }
    ch.ring.pop()
}

pub fn peek(channel_id: u32) -> bool {
    let idx = channel_id as usize;
    if idx >= MAX_CHANNELS {
        return false;
    }
    let ch = unsafe { &(*core::ptr::addr_of!(CHANNELS))[idx] };
    !ch.ring.is_empty()
}

pub fn pending(channel_id: u32) -> usize {
    let idx = channel_id as usize;
    if idx >= MAX_CHANNELS {
        return 0;
    }
    let ch = unsafe { &(*core::ptr::addr_of!(CHANNELS))[idx] };
    ch.ring.len()
}

pub fn destroy_channel(channel_id: u32) {
    let idx = channel_id as usize;
    if idx >= MAX_CHANNELS {
        return;
    }
    let ch = unsafe { &mut (*core::ptr::addr_of_mut!(CHANNELS))[idx] };
    ch.active = false;
}

static NEXT_SPINLOCK: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
pub struct SpinLock {
    locked: AtomicU32,
}

impl SpinLock {
    pub const fn new() -> Self {
        Self {
            locked: AtomicU32::new(0),
        }
    }

    pub fn lock(&self) {
        while self.locked.compare_exchange_weak(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err()
        {
            while self.locked.load(Ordering::Relaxed) != 0 {
                core::hint::spin_loop();
            }
        }
    }

    pub fn unlock(&self) {
        self.locked.store(0, Ordering::Release);
    }

    pub fn try_lock(&self) -> bool {
        self.locked.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_ok()
    }
}
