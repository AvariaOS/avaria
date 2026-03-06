
use crate::kern::mem;
use crate::kern::serial;
use super::block;
use super::superblock::Superblock;

pub const ROOT_INO: u32 = 2;

pub const S_IFMT: u16 = 0xF000;
pub const S_IFREG: u16 = 0x8000;
pub const S_IFDIR: u16 = 0x4000;
pub const S_IFLNK: u16 = 0xA000;

#[repr(C)]
pub struct Inode {
    pub i_mode: u16,
    pub i_uid: u16,
    pub i_size: u32,
    pub i_atime: u32,
    pub i_ctime: u32,
    pub i_mtime: u32,
    pub i_dtime: u32,
    pub i_gid: u16,
    pub i_links_count: u16,
    pub i_blocks: u32,
    pub i_flags: u32,
    pub i_osd1: u32,
    pub i_block: [u32; 15],
    pub i_generation: u32,
    pub i_file_acl: u32,
    pub i_size_high: u32,
    pub i_faddr: u32,
    pub i_osd2: [u8; 12],
}

#[repr(C)]
struct BlockGroupDesc {
    bg_block_bitmap: u32,
    bg_inode_bitmap: u32,
    bg_inode_table: u32,
    bg_free_blocks_count: u16,
    bg_free_inodes_count: u16,
    bg_used_dirs_count: u16,
    _pad: [u8; 14],
}

impl Inode {
    pub fn size(&self) -> u64 {
        let lo = self.i_size as u64;
        if self.i_mode & S_IFMT == S_IFREG {
            lo | ((self.i_size_high as u64) << 32)
        } else {
            lo
        }
    }

    pub fn is_dir(&self) -> bool {
        self.i_mode & S_IFMT == S_IFDIR
    }

    pub fn is_file(&self) -> bool {
        self.i_mode & S_IFMT == S_IFREG
    }
}

pub fn read_inode(port: usize, sb: &Superblock, ino: u32) -> *mut Inode {
    if ino == 0 {
        return core::ptr::null_mut();
    }

    let block_size = sb.block_size();
    let group = (ino - 1) / sb.s_inodes_per_group;
    let index = (ino - 1) % sb.s_inodes_per_group;
    let inode_size = sb.inode_size();

    let bgdt_block = if block_size == 1024 { 2 } else { 1 };
    let bgd_offset = bgdt_block as u64 * block_size as u64 + group as u64 * 32;

    let mut bgd = BlockGroupDesc {
        bg_block_bitmap: 0,
        bg_inode_bitmap: 0,
        bg_inode_table: 0,
        bg_free_blocks_count: 0,
        bg_free_inodes_count: 0,
        bg_used_dirs_count: 0,
        _pad: [0; 14],
    };

    if !block::read_bytes(
        port,
        bgd_offset,
        &mut bgd as *mut BlockGroupDesc as *mut u8,
        32,
    ) {
        serial::puts("ext3: failed to read BGD\n");
        return core::ptr::null_mut();
    }

    let inode_table_byte = bgd.bg_inode_table as u64 * block_size as u64;
    let inode_byte_off = inode_table_byte + index as u64 * inode_size as u64;

    let alloc = inode_alloc_size(inode_size);
    let buf = mem::kmalloc(alloc) as *mut u8;
    if buf.is_null() {
        return core::ptr::null_mut();
    }
    unsafe { core::ptr::write_bytes(buf, 0, alloc) };

    if !block::read_bytes(port, inode_byte_off, buf, inode_size as usize) {
        serial::puts("ext3: failed to read inode ");
        serial::dec(ino as u64);
        serial::puts("\n");
        mem::kfree(buf, alloc);
        return core::ptr::null_mut();
    }

    buf as *mut Inode
}

pub fn inode_alloc_size(inode_size: u32) -> usize {
    let min = core::mem::size_of::<Inode>();
    if (inode_size as usize) > min {
        inode_size as usize
    } else {
        min
    }
}

pub fn read_inode_data(port: usize, sb: &Superblock, inode: &Inode) -> (*mut u8, usize) {
    let size = inode.size() as usize;
    if size == 0 {
        return (core::ptr::null_mut(), 0);
    }

    let buf = mem::kmalloc(size);
    if buf.is_null() {
        return (core::ptr::null_mut(), 0);
    }

    let block_size = sb.block_size();
    let blocks_needed = (size + block_size as usize - 1) / block_size as usize;
    let mut bytes_left = size;
    let mut buf_off = 0usize;

    for logical_block in 0..blocks_needed {
        let phys_block = resolve_block(port, sb, inode, logical_block as u32);
        if phys_block == 0 {
            let chunk = core::cmp::min(bytes_left, block_size as usize);
            unsafe { core::ptr::write_bytes(buf.add(buf_off), 0, chunk) };
            buf_off += chunk;
            bytes_left -= chunk;
            continue;
        }

        let chunk = core::cmp::min(bytes_left, block_size as usize);
        if !block::read_block(port, phys_block as u64, block_size, unsafe { buf.add(buf_off) }) {
            mem::kfree(buf, size);
            return (core::ptr::null_mut(), 0);
        }
        buf_off += chunk;
        bytes_left -= chunk;
    }

    (buf, size)
}

fn resolve_block(port: usize, sb: &Superblock, inode: &Inode, logical: u32) -> u32 {
    let block_size = sb.block_size();
    let ptrs_per_block = block_size / 4;

    if logical < 12 {
        return inode.i_block[logical as usize];
    }

    let logical = logical - 12;

    if logical < ptrs_per_block {
        let indirect_block = inode.i_block[12];
        if indirect_block == 0 {
            return 0;
        }
        return read_block_ptr(port, block_size, indirect_block, logical);
    }

    let logical = logical - ptrs_per_block;

    let double_range = ptrs_per_block * ptrs_per_block;
    if logical < double_range {
        let dbl_block = inode.i_block[13];
        if dbl_block == 0 {
            return 0;
        }
        let idx1 = logical / ptrs_per_block;
        let idx2 = logical % ptrs_per_block;
        let ind_block = read_block_ptr(port, block_size, dbl_block, idx1);
        if ind_block == 0 {
            return 0;
        }
        return read_block_ptr(port, block_size, ind_block, idx2);
    }

    let logical = logical - double_range;

    let triple_range = ptrs_per_block * ptrs_per_block * ptrs_per_block;
    if logical < triple_range {
        let tri_block = inode.i_block[14];
        if tri_block == 0 {
            return 0;
        }
        let idx1 = logical / (ptrs_per_block * ptrs_per_block);
        let rem = logical % (ptrs_per_block * ptrs_per_block);
        let idx2 = rem / ptrs_per_block;
        let idx3 = rem % ptrs_per_block;

        let dbl = read_block_ptr(port, block_size, tri_block, idx1);
        if dbl == 0 {
            return 0;
        }
        let ind = read_block_ptr(port, block_size, dbl, idx2);
        if ind == 0 {
            return 0;
        }
        return read_block_ptr(port, block_size, ind, idx3);
    }

    0
}

fn read_block_ptr(port: usize, block_size: u32, block_num: u32, index: u32) -> u32 {
    let byte_off = block_num as u64 * block_size as u64 + index as u64 * 4;
    let mut val: u32 = 0;
    if !block::read_bytes(port, byte_off, &mut val as *mut u32 as *mut u8, 4) {
        return 0;
    }
    val
}
