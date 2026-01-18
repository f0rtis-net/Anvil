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