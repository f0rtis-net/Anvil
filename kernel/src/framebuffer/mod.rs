use spin::{Mutex, Once};

pub struct Framebuffer {
    buff_ptr: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    fg_color: u32,
    bg_color: u32,
    bpp: usize,
}

static FRAMEBUFFER: Once<Mutex<Framebuffer>> = Once::new();


unsafe impl Sync for Framebuffer {}
unsafe impl Send for Framebuffer {}

impl Framebuffer {
    pub fn init(
        framebuffer: *mut u8,
        width: usize,
        height: usize,
        pitch: usize,
        bpp: usize) {

        FRAMEBUFFER.call_once(|| Mutex::new(Self {
            buff_ptr: framebuffer,
            width,
            height,
            pitch,
            bpp,
            bg_color: 0x000000,
            fg_color: 0xFFFFFF
        }));
    }

    pub fn get_global() -> &'static Mutex<Framebuffer> {
        FRAMEBUFFER.get().expect("Framebuffer not initialized")
    }

    pub fn get_width(&self) -> usize{
        self.width
    }

    pub fn get_height(&self) -> usize{
        self.height
    }

    pub fn set_color(&mut self, fg: u32, bg: u32) {
        self.fg_color = fg;
        self.bg_color = bg;
    }

    pub fn draw_pixel(&self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = y * self.pitch + x * (self.bpp / 8);
        unsafe {
            let pixel = self.buff_ptr.add(offset) as *mut u32;
            *pixel = color;
        }
    }

    pub fn clear(&mut self) {
        unsafe {
            self.buff_ptr.write_bytes(0, self.pitch * self.height);
        }
    }

    pub fn scroll(&mut self, pixels: usize) {
        let line_bytes = self.pitch * pixels;
        let total_bytes = self.pitch * self.height;
        
        if pixels >= self.height {
            return;
        }
        
        unsafe {
            let src = self.buff_ptr.add(line_bytes);
            let dst = self.buff_ptr;
            let size = total_bytes - line_bytes;
            
            core::ptr::copy(src, dst, size);
            
            let clear_start = total_bytes - line_bytes;
            self.buff_ptr.add(clear_start).write_bytes(0, line_bytes);
        }
    }
}