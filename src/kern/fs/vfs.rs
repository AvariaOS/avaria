pub enum FileType {
    File,
    Directory,
}

pub struct DirEntry<'a> {
    pub name: &'a str,
    pub file_type: FileType,
    pub size: usize,
}

pub trait FileSystem {
    fn read(&self, path: &str) -> Option<&[u8]>;
    fn exists(&self, path: &str) -> bool;
    fn list(&self, path: &str) -> Option<DirIter<'_>>;
}

pub struct DirIter<'a> {
    entries: &'a [DirEntry<'a>],
    pos: usize,
}

impl<'a> DirIter<'a> {
    pub fn new(entries: &'a [DirEntry<'a>]) -> Self {
        Self { entries, pos: 0 }
    }
}

impl<'a> Iterator for DirIter<'a> {
    type Item = &'a DirEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.entries.len() {
            let entry = &self.entries[self.pos];
            self.pos += 1;
            Some(entry)
        } else {
            None
        }
    }
}
