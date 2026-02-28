#![allow(dead_code)]

use crate::arch::amd64::memory::pmm::sparsemem::{
    FrameState, PAGE_SHIFT, PAGES_PER_SECTION, Pfn, SECTION_SHIFT, SparseMem,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PfnRun {
    pub start: Pfn,
    pub len:   usize,
}

impl PfnRun {
    #[inline]
    pub fn end(&self) -> Pfn {
        self.start + self.len
    }
}

pub struct UsablePfnRunIter<'a> {
    sparse:    &'a SparseMem,
    sec:       usize,
    idx:       usize,
    run_start: Option<Pfn>,
    run_len:   usize,
}

impl<'a> UsablePfnRunIter<'a> {
    pub fn new(sparse: &'a SparseMem) -> Self {
        Self {
            sparse,
            sec:       0,
            idx:       0,
            run_start: None,
            run_len:   0,
        }
    }
}

#[inline]
fn close_run(run_start: &mut Option<Pfn>, run_len: &mut usize) -> Option<PfnRun> {
    run_start.take().map(|start| {
        let len = *run_len;
        *run_len = 0;
        PfnRun { start, len }
    })
}

impl<'a> Iterator for UsablePfnRunIter<'a> {
    type Item = PfnRun;

    fn next(&mut self) -> Option<Self::Item> {
        if self.sparse.section_count == 0 {
            return close_run(&mut self.run_start, &mut self.run_len);
        }

        let max_sec = self.sparse.max_present_sec;

        while self.sec <= max_sec {
            let sec     = self.sec;
            let section = unsafe { &*self.sparse.sections.add(sec) };

            if !section.present {
                self.idx  = 0;
                self.sec += 1;

                if let Some(run) = close_run(&mut self.run_start, &mut self.run_len) {
                    return Some(run);
                }
                continue;
            }

            let base_pfn = sec << (SECTION_SHIFT - PAGE_SHIFT);

            while self.idx < PAGES_PER_SECTION {
                let frame = unsafe { &*section.frames.add(self.idx) };
                let pfn   = base_pfn + self.idx;
                self.idx += 1;

                if frame.state == FrameState::Usable {
                    match self.run_start {
                        None => {
                            self.run_start = Some(pfn);
                            self.run_len   = 1;
                        }
                        Some(_) => {
                            self.run_len += 1;
                        }
                    }
                } else if let Some(run) = close_run(&mut self.run_start, &mut self.run_len) {
                    return Some(run);
                }
            }

            self.idx  = 0;
            self.sec += 1;
        }

        close_run(&mut self.run_start, &mut self.run_len)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.sparse.section_count == 0 {
            return (0, Some(0));
        }
        let remaining = self.sparse.max_present_sec
            .saturating_sub(self.sec)
            .saturating_add(1);
        (0, Some(remaining))
    }
}