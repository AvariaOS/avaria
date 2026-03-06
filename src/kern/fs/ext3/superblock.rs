
use crate::kern::mem;
use crate::kern::serial;
use super::block;

pub const EXT_MAGIC: u16 = 0xEF53;

#[repr(C)]
pub struct Superblock {
    pub s_inodes_count: u32,
    pub s_blocks_count: u32,
    pub s_r_blocks_count: u32,
    pub s_free_blocks_count: u32,
    pub s_free_inodes_count: u32,
    pub s_first_data_block: u32,
    pub s_log_block_size: u32,
    pub s_log_frag_size: u32,
    pub s_blocks_per_group: u32,
    pub s_frags_per_group: u32,
    pub s_inodes_per_group: u32,
    pub s_mtime: u32,
    pub s_wtime: u32,
    pub s_mnt_count: u16,
    pub s_max_mnt_count: u16,
    pub s_magic: u16,
    pub s_state: u16,
    pub s_errors: u16,
    pub s_minor_rev_level: u16,
    pub s_lastcheck: u32,
    pub s_checkinterval: u32,
    pub s_creator_os: u32,
    pub s_rev_level: u32,
    pub s_def_resuid: u16,
    pub s_def_resgid: u16,
    pub s_first_ino: u32,
    pub s_inode_size: u16,
    pub s_block_group_nr: u16,
    pub s_feature_compat: u32,
    pub s_feature_incompat: u32,
    pub s_feature_ro_compat: u32,
    pub s_uuid: [u8; 16],
    pub s_volume_name: [u8; 16],
    _pad: [u8; 0x400 - 0x88],
}

impl Superblock {
    pub fn block_size(&self) -> u32 {
        1024 << self.s_log_block_size
    }

    pub fn block_group_count(&self) -> u32 {
        (self.s_blocks_count + self.s_blocks_per_group - 1) / self.s_blocks_per_group
    }

    pub fn inode_size(&self) -> u32 {
        if self.s_rev_level >= 1 && self.s_inode_size > 0 {
            self.s_inode_size as u32
        } else {
            128
        }
    }
}

pub fn read_superblock(port: usize) -> *mut Superblock {
    let buf = mem::kmalloc_aligned(4096, 4096);
    if buf.is_null() {
        serial::puts("ext3: cannot alloc superblock buf\n");
        return core::ptr::null_mut();
    }

    if !block::read_bytes(port, 1024, buf, 1024) {
        serial::puts("ext3: failed to read superblock\n");
        mem::kfree(buf, 4096);
        return core::ptr::null_mut();
    }

    let sb = buf as *mut Superblock;
    let magic = unsafe { (*sb).s_magic };
    if magic != EXT_MAGIC {
        serial::puts("ext3: bad magic 0x");
        serial::hex(magic as u64);
        serial::puts("\n");
        mem::kfree(buf, 4096);
        return core::ptr::null_mut();
    }

    sb
}
