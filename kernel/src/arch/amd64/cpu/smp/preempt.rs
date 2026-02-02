use core::{ops::{Deref, DerefMut}};

use crate::define_per_cpu_u64;

define_per_cpu_u64!(PREEMPT_COUNT);

#[inline]
pub(crate) fn barrier() {
    unsafe {
        core::arch::asm!(
            "mfence",
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct PreemptGuard<T> {
    val: T,
}

impl<T: Copy> PreemptGuard<T> {
    #[allow(dead_code)]
    pub(crate) fn map<U, F: FnOnce(T) -> U>(self, f: F) -> PreemptGuard<U> {
        let val = self.val;
        PreemptGuard::new(f(val))
    }
}

impl<T> PreemptGuard<T> {
    pub(crate) fn new(val: T) -> Self {
        inc_per_cpu_PREEMPT_COUNT();
        barrier();
        Self { val }
    }
}

impl<T> Drop for PreemptGuard<T> {
    fn drop(&mut self) {
        barrier();
        dec_per_cpu_PREEMPT_COUNT();
    }
}

impl<T> Deref for PreemptGuard<T> {
    type Target = T;

    #[allow(clippy::explicit_deref_methods)]
    fn deref(&self) -> &T {
        &self.val
    }
}

impl<T> DerefMut for PreemptGuard<T> {
    #[allow(clippy::explicit_deref_methods)]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.val
    }
}