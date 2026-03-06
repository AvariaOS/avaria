
use crate::kern::serial;
use super::inode::{self, Inode, S_IFMT, S_IFDIR};
use super::superblock::Superblock;

#[repr(C)]
pub struct DirEntry {
    pub inode: u32,
    pub rec_len: u16,
    pub name_len: u8,
    pub file_type: u8,
}

pub const FT_UNKNOWN: u8 = 0;
pub const FT_REG_FILE: u8 = 1;
pub const FT_DIR: u8 = 2;
pub const FT_SYMLINK: u8 = 7;

pub fn lookup(
    port: usize,
    sb: &Superblock,
    dir_inode: &Inode,
    name: &[u8],
) -> u32 {
    if dir_inode.i_mode & S_IFMT != S_IFDIR {
        return 0;
    }

    let (data, size) = inode::read_inode_data(port, sb, dir_inode);
    if data.is_null() || size == 0 {
        return 0;
    }

    let result = find_in_dir_data(data, size, name);

    crate::kern::mem::kfree(data, size);
    result
}

fn find_in_dir_data(data: *const u8, size: usize, name: &[u8]) -> u32 {
    let mut off = 0usize;

    while off + 8 <= size {
        let entry = unsafe { &*(data.add(off) as *const DirEntry) };

        if entry.rec_len == 0 {
            break;
        }

        if entry.inode != 0 && entry.name_len as usize == name.len() {
            let entry_name = unsafe {
                core::slice::from_raw_parts(data.add(off + 8), entry.name_len as usize)
            };
            if entry_name == name {
                return entry.inode;
            }
        }

        off += entry.rec_len as usize;
    }

    0
}

pub fn resolve_path(port: usize, sb: &Superblock, start_ino: u32, path: &str) -> u32 {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return start_ino;
    }

    let mut current_ino = start_ino;

    for component in path.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }

        let ino_ptr = inode::read_inode(port, sb, current_ino);
        if ino_ptr.is_null() {
            return 0;
        }

        let ino_ref = unsafe { &*ino_ptr };
        let found = lookup(port, sb, ino_ref, component.as_bytes());

        let alloc = inode::inode_alloc_size(sb.inode_size());
        crate::kern::mem::kfree(ino_ptr as *mut u8, alloc);

        if found == 0 {
            return 0;
        }
        current_ino = found;
    }

    current_ino
}

pub fn for_each_entry(
    port: usize,
    sb: &Superblock,
    dir_inode: &Inode,
    mut f: impl FnMut(&[u8], u32, u8),
) {
    if dir_inode.i_mode & S_IFMT != S_IFDIR {
        return;
    }

    let (data, size) = inode::read_inode_data(port, sb, dir_inode);
    if data.is_null() || size == 0 {
        return;
    }

    let mut off = 0usize;
    while off + 8 <= size {
        let entry = unsafe { &*(data.add(off) as *const DirEntry) };

        if entry.rec_len == 0 {
            break;
        }

        if entry.inode != 0 {
            let entry_name = unsafe {
                core::slice::from_raw_parts(data.add(off + 8), entry.name_len as usize)
            };
            f(entry_name, entry.inode, entry.file_type);
        }

        off += entry.rec_len as usize;
    }

    crate::kern::mem::kfree(data, size);
}
