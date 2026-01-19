#[inline]
pub const fn align_up(x: usize, a: usize) -> usize {
    (x + a - 1) & !(a - 1)
}

#[inline]
pub const fn align_down(x: usize, a: usize) -> usize {
    x & !(a - 1)
}

#[inline]
pub fn floor_log2(x: usize) -> usize {
    usize::BITS as usize - 1 - x.leading_zeros() as usize
}

#[inline]
pub fn virt_to_phys(offset: usize, virt: usize) -> usize {
    return virt - offset;
}

#[inline]
pub fn phys_to_virt(offset: usize, phys: usize) -> usize {
    return phys + offset;
}

#[inline]
pub fn pages_to_order(pages: usize) -> usize {
    assert!(pages > 0);
    let mut order = 0;
    let mut n = 1usize;
    while n < pages {
        n <<= 1;
        order += 1;
    }
    order
}

pub struct HumanSize {
    pub value: u64,
    pub unit: SizeUnit,
}

#[derive(Clone, Copy, Debug)]
pub enum SizeUnit {
    Bytes,
    KiB,
    MiB,
    GiB,
    TiB,
}

impl SizeUnit {
    pub const fn as_str(self) -> &'static str {
        match self {
            SizeUnit::Bytes => "B",
            SizeUnit::KiB   => "KiB",
            SizeUnit::MiB   => "MiB",
            SizeUnit::GiB   => "GiB",
            SizeUnit::TiB   => "TiB",
        }
    }
}

pub fn human_readable_size(bytes: u64) -> HumanSize {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    const TIB: u64 = 1024 * GIB;

    if bytes >= TIB {
        HumanSize {
            value: bytes / TIB,
            unit: SizeUnit::TiB,
        }
    } else if bytes >= GIB {
        HumanSize {
            value: bytes / GIB,
            unit: SizeUnit::GiB,
        }
    } else if bytes >= MIB {
        HumanSize {
            value: bytes / MIB,
            unit: SizeUnit::MiB,
        }
    } else if bytes >= KIB {
        HumanSize {
            value: bytes / KIB,
            unit: SizeUnit::KiB,
        }
    } else {
        HumanSize {
            value: bytes,
            unit: SizeUnit::Bytes,
        }
    }
}