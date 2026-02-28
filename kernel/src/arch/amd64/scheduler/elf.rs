use alloc::{string::String, vec::Vec};
use bitflags::bitflags;
use elf::{ElfBytes, ParseError, endian::AnyEndian, segment::ProgramHeader};
use x86_64::{VirtAddr, structures::paging::PageTableFlags};

pub struct ElfParsed<'a> {
    pub parsed: ElfBytes<'a, AnyEndian>,
    pub entrypoint: VirtAddr, 
    pub segments: Vec<LoadableSegment>
}

#[derive(Debug)]
pub(crate) enum ElfParserErrors {
    ParseError(ParseError),
    Other,
}

pub struct LoadableSegment {
    pub raw_header: ProgramHeader,
    pub file_offset: u64,
    pub vaddr: VirtAddr,
    pub mem_size: u64,
    pub flags: LoadableSegmentFlags,
    pub alignment: u64,
}

bitflags! {
    #[derive(Debug)]
    #[repr(transparent)]
    pub(super) struct LoadableSegmentFlags: u32 {
        const EXECUTABLE = 1;
        const WRITABLE = 2;
        const READABLE = 4;
    }
}

impl LoadableSegmentFlags {
    pub fn page_table_entry_flags(&self) -> PageTableFlags {
        let mut flags = PageTableFlags::USER_ACCESSIBLE;

        if !self.contains(Self::EXECUTABLE) {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        if self.contains(Self::WRITABLE) {
            flags |= PageTableFlags::WRITABLE;
        }

        if self.contains(Self::READABLE) {
            flags |= PageTableFlags::PRESENT;
        }

        flags
    }
}

impl<'a> ElfParsed<'a> {
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        let parsed = ElfBytes::<AnyEndian>::minimal_parse(bytes).unwrap();

        if parsed.ehdr.e_type != elf::abi::ET_EXEC {
            return None;
        }

        if parsed.ehdr.e_machine != elf::abi::EM_X86_64 {
            return None;
        }

        let entrypoint = VirtAddr::new(parsed.ehdr.e_entry);

        let Some(segments) = parsed
            .segments() else {
                return None;
            };

        let mut loadable_segments = Vec::new();
        for program_header in segments {
            if program_header.p_type != elf::abi::PT_LOAD {
                continue;
            }

            if program_header.p_paddr > 0 && program_header.p_paddr != program_header.p_vaddr {
                return None;
            }

            let file_offset = program_header.p_offset;
            let vaddr = VirtAddr::new(program_header.p_vaddr);
            let mem_size = program_header.p_memsz;
            let flags =
                LoadableSegmentFlags::from_bits(program_header.p_flags).unwrap();

            let alignment = program_header.p_align;

            loadable_segments.push(LoadableSegment {
                raw_header: program_header,
                file_offset,
                vaddr,
                mem_size,
                flags,
                alignment,
            });
        }

        Some(Self {
            parsed,
            entrypoint,
            segments: loadable_segments,
        })
    }
}
