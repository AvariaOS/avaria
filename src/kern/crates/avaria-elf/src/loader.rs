use crate::header::*;

pub struct LoadedElf {
    pub entry: u64,
    pub load_base: u64,
    pub load_size: usize,
}

pub fn load_at(data: &[u8], buffer: &mut [u8]) -> Result<LoadedElf, ElfError> {
    let hdr = parse_header(data)?;
    let phdrs = program_headers(data, hdr)?;

    let mut vaddr_min = u64::MAX;
    let mut vaddr_max = 0u64;
    for phdr in phdrs {
        if phdr.p_type != PT_LOAD {
            continue;
        }
        if phdr.p_vaddr < vaddr_min {
            vaddr_min = phdr.p_vaddr;
        }
        let end = phdr.p_vaddr + phdr.p_memsz;
        if end > vaddr_max {
            vaddr_max = end;
        }
    }

    if vaddr_min == u64::MAX {
        return Err(ElfError::BadPhdr);
    }

    let total_size = (vaddr_max - vaddr_min) as usize;
    if total_size > buffer.len() {
        return Err(ElfError::BadPhdr);
    }

    buffer[..total_size].fill(0);

    let base = buffer.as_ptr() as u64;
    let reloc_offset = base.wrapping_sub(vaddr_min);

    for phdr in phdrs {
        if phdr.p_type != PT_LOAD {
            continue;
        }
        let src_off = phdr.p_offset as usize;
        let filesz = phdr.p_filesz as usize;
        let dst_off = (phdr.p_vaddr - vaddr_min) as usize;

        if src_off + filesz > data.len() {
            return Err(ElfError::BadPhdr);
        }

        buffer[dst_off..dst_off + filesz].copy_from_slice(&data[src_off..src_off + filesz]);
    }

    Ok(LoadedElf {
        entry: hdr.e_entry.wrapping_add(reloc_offset),
        load_base: base,
        load_size: total_size,
    })
}

impl LoadedElf {
    pub unsafe fn call(&self, api: *const ()) -> i32 {
        let entry: unsafe extern "C" fn(*const ()) -> i32 =
            unsafe { core::mem::transmute(self.entry) };
        unsafe { entry(api) }
    }

    pub unsafe fn jump(&self) -> ! {
        let entry: unsafe extern "C" fn() -> ! = unsafe { core::mem::transmute(self.entry) };
        unsafe { entry() }
    }
}
