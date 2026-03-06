
use crate::kern::drv::disk::ahci;
use crate::kern::mem;
use core::ptr;

pub fn read_bytes(port: usize, byte_off: u64, dst: *mut u8, count: usize) -> bool {
    if count == 0 {
        return true;
    }

    let sector_start = byte_off / 512;
    let offset_in_sector = (byte_off % 512) as usize;
    let total_bytes = offset_in_sector + count;
    let sector_count = (total_bytes + 511) / 512;

    let buf_size = sector_count * 512;
    let buf = mem::kmalloc_aligned(buf_size, 4096);
    if buf.is_null() {
        return false;
    }

    let ok = ahci::read_sectors(port, sector_start, sector_count as u16, buf);
    if ok {
        unsafe {
            ptr::copy_nonoverlapping(buf.add(offset_in_sector), dst, count);
        }
    }

    mem::kfree(buf, buf_size);
    ok
}

pub fn read_block(port: usize, block_num: u64, block_size: u32, dst: *mut u8) -> bool {
    let byte_off = block_num * block_size as u64;
    read_bytes(port, byte_off, dst, block_size as usize)
}
