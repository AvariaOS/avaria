use super::vesa::Framebuffer;

const PSF1_MAGIC: [u8; 2] = [0x36, 0x04];
const MAX_UNICODE_MAPPINGS: usize = 512;

struct UnicodeMap {
    entries: [(u16, u16); MAX_UNICODE_MAPPINGS],
    len: usize,
}

impl UnicodeMap {
    fn new() -> Self {
        Self {
            entries: [(0, 0); MAX_UNICODE_MAPPINGS],
            len: 0,
        }
    }

    fn insert(&mut self, codepoint: u16, glyph: u16) {
        if self.len < MAX_UNICODE_MAPPINGS {
            self.entries[self.len] = (codepoint, glyph);
            self.len += 1;
        }
    }

    fn lookup(&self, codepoint: u16) -> Option<u16> {
        for i in 0..self.len {
            if self.entries[i].0 == codepoint {
                return Some(self.entries[i].1);
            }
        }
        None
    }
}

pub struct PsfFont<'a> {
    glyphs: &'a [u8],
    pub charsize: usize,
    pub width: usize,
    pub height: usize,
    num_glyphs: usize,
    unicode_map: UnicodeMap,
}

impl<'a> PsfFont<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < 4 || data[0] != PSF1_MAGIC[0] || data[1] != PSF1_MAGIC[1] {
            return None;
        }
        let mode = data[2];
        let charsize = data[3] as usize;
        let num_glyphs = if mode & 0x01 != 0 { 512 } else { 256 };
        let glyph_data_len = num_glyphs * charsize;
        if data.len() < 4 + glyph_data_len {
            return None;
        }

        let mut unicode_map = UnicodeMap::new();
        let has_unicode = mode & 0x02 != 0;

        if has_unicode {
            let table = &data[4 + glyph_data_len..];
            let mut off = 0;
            let mut glyph_idx: u16 = 0;
            while off + 1 < table.len() && (glyph_idx as usize) < num_glyphs {
                let v = (table[off] as u16) | ((table[off + 1] as u16) << 8);
                off += 2;
                if v == 0xFFFF {
                    glyph_idx += 1;
                } else if v == 0xFFFE {
                    continue;
                } else {
                    unicode_map.insert(v, glyph_idx);
                }
            }
        }

        Some(Self {
            glyphs: &data[4..4 + glyph_data_len],
            charsize,
            width: 8,
            height: charsize,
            num_glyphs,
            unicode_map,
        })
    }

    fn glyph_index(&self, codepoint: u32) -> usize {
        if codepoint < 0x80 && (codepoint as usize) < self.num_glyphs {
            return codepoint as usize;
        }
        if let Some(idx) = self.unicode_map.lookup(codepoint as u16) {
            return idx as usize;
        }
        b'?' as usize
    }

    pub fn draw_char(&self, fb: &Framebuffer, x: usize, y: usize, codepoint: u32, fg: u32, bg: u32) {
        let idx = self.glyph_index(codepoint);
        if idx >= self.num_glyphs {
            return;
        }
        let glyph = &self.glyphs[idx * self.charsize..(idx + 1) * self.charsize];
        let fg_r = ((fg >> 16) & 0xFF) as u8;
        let fg_g = ((fg >> 8) & 0xFF) as u8;
        let fg_b = (fg & 0xFF) as u8;
        let bg_r = ((bg >> 16) & 0xFF) as u8;
        let bg_g = ((bg >> 8) & 0xFF) as u8;
        let bg_b = (bg & 0xFF) as u8;

        for row in 0..self.height {
            let bits = glyph[row];
            for col in 0..self.width {
                if bits & (0x80 >> col) != 0 {
                    fb.put_pixel(x + col, y + row, fg_r, fg_g, fg_b);
                } else {
                    fb.put_pixel(x + col, y + row, bg_r, bg_g, bg_b);
                }
            }
        }
    }

    pub fn draw_str(&self, fb: &Framebuffer, x: usize, y: usize, s: &str, fg: u32, bg: u32) {
        let mut cx = x;
        let mut cy = y;
        for ch in s.chars() {
            if ch == '\n' {
                cx = x;
                cy += self.height;
                continue;
            }
            if cx + self.width > fb.width {
                cx = x;
                cy += self.height;
            }
            if cy + self.height > fb.height {
                break;
            }
            self.draw_char(fb, cx, cy, ch as u32, fg, bg);
            cx += self.width;
        }
    }
}
