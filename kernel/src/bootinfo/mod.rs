use limine::{framebuffer::Framebuffer, memory_map::Entry, mp, response::{ModuleResponse, MpResponse}};
use spin::{Once, RwLock, RwLockReadGuard};

use crate::bootinfo::requests::{BASE_REVISION, FRAMEBUFFER_REQUEST, HHDM_REQUEST, MEMMAP_REQUEST, MODULE_REQUEST, RSDP_REQUEST, SMP_REQUEST};

mod requests;

static BOOT_PARAMS: Once<RwLock<BootInfo>> = Once::new();

pub type MemmapEntries = &'static[&'static Entry];

pub struct BootInfo {
    bootloader_supported: bool,
    framebuffer: Option<Framebuffer<'static>>,
    rsdp_addr: Option<usize>,
    hhdm_offset: Option<u64>,
    memmap_entries: Option<MemmapEntries>,
    init_srvs: Option<&'static ModuleResponse>
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
            init_srvs: MODULE_REQUEST.get_response()
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

    pub fn get_init_srvs() -> Option<&'static [u8]> {
        let response = MODULE_REQUEST.get_response()?;
        for module in response.modules() {
            let cmdline = core::str::from_utf8(module.string().to_bytes())
                .unwrap_or("");
            if cmdline.contains("init_srvs") || cmdline.is_empty() {
                let addr = module.addr() as *const u8;
                let size = module.size() as usize;
                return Some(unsafe { core::slice::from_raw_parts(addr, size) });
            }
        }
        None
    }
}