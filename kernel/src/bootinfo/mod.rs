use limine::{framebuffer::Framebuffer, memory_map::Entry, mp, response::MpResponse};
use spin::{Once, RwLock, RwLockReadGuard};

use crate::bootinfo::requests::{BASE_REVISION, FRAMEBUFFER_REQUEST, HHDM_REQUEST, MEMMAP_REQUEST, RSDP_REQUEST, SMP_REQUEST};

mod requests;

static BOOT_PARAMS: Once<RwLock<BootInfo>> = Once::new();

pub type MemmapEntries = &'static[&'static Entry];

pub struct BootInfo {
    bootloader_supported: bool,
    framebuffer: Option<Framebuffer<'static>>,
    rsdp_addr: Option<usize>,
    hhdm_offset: Option<u64>,
    memmap_entries: Option<MemmapEntries>,
}

impl BootInfo {
    pub fn init() {
        BOOT_PARAMS.call_once(|| {
            RwLock::new(BootInfo::parse_params())
        });
    }

    fn parse_params() -> Self {
        Self {
            bootloader_supported: BASE_REVISION.is_supported(),
            framebuffer: FRAMEBUFFER_REQUEST.get_response().unwrap().framebuffers().next(),
            rsdp_addr: RSDP_REQUEST.get_response().map(|addr| addr.address()),
            hhdm_offset: HHDM_REQUEST.get_response().map(|offset| offset.offset()),
            memmap_entries: MEMMAP_REQUEST.get_response().map(|resp| resp.entries()),
        }
    }

    pub fn get() -> RwLockReadGuard<'static, BootInfo> {
        BOOT_PARAMS.get().expect("Can not get boot params. Maybe uninitialized!").read()
    }
}

impl BootInfo {
    pub fn framebuffer(&self) -> Option<&Framebuffer<'static>> {
        self.framebuffer.as_ref()
    }
    
    pub fn rsdp_addr(&self) -> Option<usize> {
        self.rsdp_addr
    }
    
    pub fn hhdm_offset(&self) -> Option<u64> {
        self.hhdm_offset
    }
    
    pub fn memmap_entries(&self) -> Option<MemmapEntries> {
        self.memmap_entries
    }
    
    pub fn bootloader_supported(&self) -> bool {
        self.bootloader_supported
    }

    pub fn get_smp_response(&self) -> Option<&'static MpResponse> {
        SMP_REQUEST.get_response()
    }
}