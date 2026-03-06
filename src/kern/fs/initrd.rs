use super::vfs::{DirIter, FileSystem};

const MAX_ENTRIES: usize = 64;

struct TarEntry<'a> {
    name: &'a str,
    data: &'a [u8],
    is_dir: bool,
}

pub struct TarFs<'a> {
    entries: [Option<TarEntry<'a>>; MAX_ENTRIES],
    count: usize,
}

impl<'a> TarFs<'a> {
    pub fn parse(archive: &'a [u8]) -> Option<Self> {
        let mut entries: [Option<TarEntry<'a>>; MAX_ENTRIES] = [const { None }; MAX_ENTRIES];
        let mut count = 0;
        let mut offset = 0;

        while offset + 512 <= archive.len() && count < MAX_ENTRIES {
            if archive[offset] == 0 {
                break;
            }
            let header = &archive[offset..offset + 512];
            let name_end = header[..100].iter().position(|&b| b == 0).unwrap_or(100);
            let name = core::str::from_utf8(&header[..name_end]).ok()?;
            let name = name.trim_start_matches("./");
            if name.is_empty() {
                let size = parse_octal(&header[124..136]);
                let blocks = (size + 511) / 512;
                offset += 512 + blocks * 512;
                continue;
            }
            let typeflag = header[156];
            let is_dir = typeflag == b'5' || name.ends_with('/');
            let size = if is_dir { 0 } else { parse_octal(&header[124..136]) };
            let data_start = offset + 512;

            entries[count] = Some(TarEntry {
                name,
                data: if is_dir { &[] } else { &archive[data_start..data_start + size] },
                is_dir,
            });
            count += 1;

            let blocks = (size + 511) / 512;
            offset = data_start + blocks * 512;
        }

        Some(Self { entries, count })
    }

    fn find(&self, path: &str) -> Option<&TarEntry<'a>> {
        let path = path.trim_start_matches('/');
        for i in 0..self.count {
            if let Some(entry) = &self.entries[i] {
                let entry_name = entry.name.trim_end_matches('/');
                if entry_name == path {
                    return Some(entry);
                }
            }
        }
        None
    }
}

impl<'a> TarFs<'a> {
    pub fn for_each_matching(&self, prefix: &str, suffix: &str, mut f: impl FnMut(&str, &[u8])) {
        let prefix = prefix.trim_start_matches('/');
        for i in 0..self.count {
            if let Some(entry) = &self.entries[i] {
                if entry.is_dir {
                    continue;
                }
                let name = entry.name.trim_end_matches('/');
                if name.starts_with(prefix) && name.ends_with(suffix) {
                    f(name, entry.data);
                }
            }
        }
    }
}

impl<'a> FileSystem for TarFs<'a> {
    fn read(&self, path: &str) -> Option<&[u8]> {
        let entry = self.find(path)?;
        if entry.is_dir {
            return None;
        }
        Some(entry.data)
    }

    fn exists(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    fn list(&self, _path: &str) -> Option<DirIter<'_>> {
        None
    }
}

fn parse_octal(data: &[u8]) -> usize {
    let mut val = 0usize;
    for &b in data {
        if b == 0 || b == b' ' {
            break;
        }
        val = val * 8 + (b - b'0') as usize;
    }
    val
}
