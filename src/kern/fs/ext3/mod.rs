
pub mod block;
pub mod superblock;
pub mod inode;
pub mod dir;

use crate::kern::mem;
use crate::kern::serial;
use superblock::Superblock;
use inode::{Inode, ROOT_INO, S_IFMT, S_IFDIR, S_IFREG};

pub struct Ext3 {
    port: usize,
    sb: *mut Superblock,
}

impl Ext3 {
    pub fn mount(port: usize) -> Option<Self> {
        let sb = superblock::read_superblock(port);
        if sb.is_null() {
            return None;
        }

        let sb_ref = unsafe { &*sb };
        serial::puts("ext3: mounted — ");
        serial::dec(sb_ref.s_blocks_count as u64);
        serial::puts(" blocks, ");
        serial::dec(sb_ref.block_size() as u64);
        serial::puts("B/block, ");
        serial::dec(sb_ref.s_inodes_count as u64);
        serial::puts(" inodes\n");

        let label = &sb_ref.s_volume_name;
        let label_len = label.iter().position(|&b| b == 0).unwrap_or(16);
        if label_len > 0 {
            serial::puts("ext3: label \"");
            for &b in &label[..label_len] {
                serial::putb(b);
            }
            serial::puts("\"\n");
        }

        Some(Self { port, sb })
    }

    fn sb(&self) -> &Superblock {
        unsafe { &*self.sb }
    }

    pub fn read_file(&self, path: &str) -> (*mut u8, usize) {
        let ino_num = dir::resolve_path(self.port, self.sb(), ROOT_INO, path);
        if ino_num == 0 {
            return (core::ptr::null_mut(), 0);
        }

        let ino_ptr = inode::read_inode(self.port, self.sb(), ino_num);
        if ino_ptr.is_null() {
            return (core::ptr::null_mut(), 0);
        }

        let ino_ref = unsafe { &*ino_ptr };
        if !ino_ref.is_file() {
            let alloc = inode::inode_alloc_size(self.sb().inode_size());
            mem::kfree(ino_ptr as *mut u8, alloc);
            return (core::ptr::null_mut(), 0);
        }

        let result = inode::read_inode_data(self.port, self.sb(), ino_ref);

        let alloc = inode::inode_alloc_size(self.sb().inode_size());
        mem::kfree(ino_ptr as *mut u8, alloc);

        result
    }

    pub fn exists(&self, path: &str) -> bool {
        dir::resolve_path(self.port, self.sb(), ROOT_INO, path) != 0
    }

    pub fn list_dir(&self, path: &str, f: impl FnMut(&[u8], u32, u8)) {
        let ino_num = if path.is_empty() || path == "/" {
            ROOT_INO
        } else {
            let n = dir::resolve_path(self.port, self.sb(), ROOT_INO, path);
            if n == 0 {
                return;
            }
            n
        };

        let ino_ptr = inode::read_inode(self.port, self.sb(), ino_num);
        if ino_ptr.is_null() {
            return;
        }

        let ino_ref = unsafe { &*ino_ptr };
        dir::for_each_entry(self.port, self.sb(), ino_ref, f);

        let alloc = inode::inode_alloc_size(self.sb().inode_size());
        mem::kfree(ino_ptr as *mut u8, alloc);
    }

    pub fn dump_root(&self) {
        serial::puts("ext3: / listing:\n");
        self.list_dir("/", |name, ino, ft| {
            serial::puts("  ");
            match ft {
                dir::FT_DIR => serial::puts("[DIR] "),
                dir::FT_REG_FILE => serial::puts("[FIL] "),
                dir::FT_SYMLINK => serial::puts("[LNK] "),
                _ => serial::puts("[???] "),
            }
            for &b in name {
                serial::putb(b);
            }
            serial::puts(" (ino=");
            serial::dec(ino as u64);
            serial::puts(")\n");
        });
    }
}

impl Drop for Ext3 {
    fn drop(&mut self) {
        if !self.sb.is_null() {
            mem::kfree(self.sb as *mut u8, 4096);
            self.sb = core::ptr::null_mut();
        }
    }
}
