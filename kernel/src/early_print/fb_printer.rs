use core::fmt;

use spin::{Mutex, Once};

use crate::framebuffer::Framebuffer;

pub static RENDERER: Once<Mutex<ScrollingFbTextRenderer>> = Once::new();

#[repr(C, packed)]
struct PSF1Header {
    magic: [u8; 2],
    mode: u8,
    charsize: u8,
}

#[repr(C, packed)]
struct PSF2Header {
    magic: [u8; 4],
    version: u32,
    headersize: u32,
    flags: u32,
    numglyph: u32,
    bytesperglyph: u32,
    height: u32,
    width: u32,
}

pub struct ScrollingFbTextRenderer {
    x: usize,
    y: usize,
    font_data: &'static [u8],
    char_width: usize,
    char_height: usize,
    bytes_per_glyph: usize,
    fb: &'static Mutex<Framebuffer>,
}

impl ScrollingFbTextRenderer {

    pub fn init(
        font_data: &'static [u8],
        fb: &'static Mutex<Framebuffer>
    ) {
        let (char_width, char_height, bytes_per_glyph) = Self::parse_psf(font_data);
        
        RENDERER.call_once(|| Mutex::new(Self {
            x: 0,
            y: 0,
            font_data,
            char_width,
            char_height,
            bytes_per_glyph,
            fb
        }));
    }

    fn parse_psf(data: &[u8]) -> (usize, usize, usize) {
        if data.len() >= 32 && &data[0..4] == b"\x72\xb5\x4a\x86" {
            let header = unsafe { &*(data.as_ptr() as *const PSF2Header) };
            return (
                header.width as usize,
                header.height as usize,
                header.bytesperglyph as usize,
            );
        }
        
        if data.len() >= 4 && &data[0..2] == b"\x36\x04" {
            let header = unsafe { &*(data.as_ptr() as *const PSF1Header) };
            let height = header.charsize as usize;
            let width = 8;
            let bytes_per_glyph = height;
            return (width, height, bytes_per_glyph);
        }
        
        (8, 16, 16)
    }

    fn header_size(&self) -> usize {
        if self.font_data.len() >= 32 && &self.font_data[0..4] == b"\x72\xb5\x4a\x86" {
            let header = unsafe { &*(self.font_data.as_ptr() as *const PSF2Header) };
            header.headersize as usize
        } else {
            4
        }
    }

    fn get_glyph_offset(&self, ch: char) -> usize {
        let idx = ch as usize;
        let max_glyphs = (self.font_data.len() - self.header_size()) / self.bytes_per_glyph;
        
        let glyph_idx = if idx < max_glyphs { idx } else { 0 };
        self.header_size() + glyph_idx * self.bytes_per_glyph
    }

    fn draw_char(&self, ch: char, x: usize, y: usize, fb: &mut Framebuffer) {
        let glyph_offset = self.get_glyph_offset(ch);
        let glyph_data = &self.font_data[glyph_offset..glyph_offset + self.bytes_per_glyph];
        
        let bytes_per_line = (self.char_width + 7) / 8;

        for row in 0..self.char_height {
            let line_offset = row * bytes_per_line;
            
            for col in 0..self.char_width {
                let byte_idx = line_offset + (col / 8);
                let bit_idx = 7 - (col % 8);
                
                if byte_idx < glyph_data.len() {
                    let bit = (glyph_data[byte_idx] >> bit_idx) & 1;
                    let color = if bit == 1 { 0xFFFFFF } else { 0x000000 };
                    fb.draw_pixel(x + col, y + row, color);
                }
            }
        }
    }

    fn newline(&mut self, fb: &mut Framebuffer) {
        self.x = 0;
        self.y += self.char_height;

        let fb_height = fb.get_height();

        if self.y + self.char_height > fb_height {
            fb.scroll(self.char_height);
            self.y -= self.char_height;
        }
    }

    pub fn write_char(&mut self, ch: char) {
        let mut fb_guard = self.fb.lock();
        let fb_width = fb_guard.get_width();

        match ch {
            '\n' => {
                self.newline(&mut fb_guard);
            }
            '\r' => {
                self.x = 0;
            }
            '\t' => {
                let tab_width = self.char_width * 4;
                self.x = ((self.x + tab_width) / tab_width) * tab_width;

                if self.x >= fb_width {
                    self.newline(&mut fb_guard);
                }
            }
            _ => {
                if self.x + self.char_width > fb_width {
                    self.newline(&mut fb_guard);
                }

                self.draw_char(ch, self.x, self.y, &mut fb_guard);
                self.x += self.char_width;
            }
        }
    }


    pub fn write_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.write_char(ch);
        }
    }
}

impl fmt::Write for ScrollingFbTextRenderer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_str(s);
        Ok(())
    }
}
