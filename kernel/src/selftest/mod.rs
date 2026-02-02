pub struct SelfTestMask {
    pub pmm: bool,
    pub vmm: bool,
    pub slab: bool,
}

impl SelfTestMask {
    pub fn default() -> Self {
        Self {
            pmm: false,
            vmm: false,
            slab: false
        }
    }
}