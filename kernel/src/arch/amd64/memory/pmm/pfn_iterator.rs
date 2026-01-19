use crate::{arch::amd64::memory::pmm::sparsemem::{
    FrameState, PAGE_SHIFT, PAGES_PER_SECTION, Pfn, SECTION_SHIFT, SparseMem,
}, serial_println};

#[derive(Debug, Clone, Copy)]
pub struct PfnRun {
    pub start: Pfn,
    pub len: usize,
}

pub struct PfnRunIter<'a> {
    sparse: &'a SparseMem,
    sec: usize,
    idx: usize,
    run_start: Option<Pfn>,
    run_len: usize,
}

impl<'a> PfnRunIter<'a> {
    pub fn new(sparse: &'a SparseMem) -> Self {
        Self {
            sparse,
            sec: 0,
            idx: 0,
            run_start: None,
            run_len: 0,
        }
    }

    #[inline]
    fn close_run(&mut self) -> Option<PfnRun> {
        if let Some(start) = self.run_start.take() {
            let len = self.run_len;
            self.run_len = 0;
            Some(PfnRun { start, len })
        } else {
            None
        }
    }
}

impl<'a> Iterator for PfnRunIter<'a> {
    type Item = PfnRun;

    fn next(&mut self) -> Option<Self::Item> {
        let max_sec = self.sparse.max_present_sec;

        while self.sec <= max_sec {
        
            let section = &self.sparse.sections()[self.sec];

            // ===== absent section =====
            if !section.present {
                self.idx = 0;
                self.sec += 1;

                if let Some(run) = self.close_run() {
                    return Some(run);
                }
                continue;
            }

            let base_pfn = self.sec << (SECTION_SHIFT - PAGE_SHIFT);

            // ===== scan section =====
            while self.idx < PAGES_PER_SECTION {
                let pfn = base_pfn + self.idx;
                let frame = unsafe { &*section.frames.add(self.idx) };
                self.idx += 1;

                if frame.state == FrameState::Usable {
                    if self.run_start.is_none() {
                        self.run_start = Some(pfn);
                        self.run_len = 1;
                    } else {
                        self.run_len += 1;
                    }
                } else if let Some(run) = self.close_run() {
                    return Some(run);
                }
            }

            // ===== end of section =====
            self.idx = 0;
            self.sec += 1;

            if let Some(run) = self.close_run() {
                return Some(run);
            }
        }

        None
    }
}
