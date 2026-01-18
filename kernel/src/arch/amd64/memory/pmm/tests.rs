use crate::{arch::amd64::memory::pmm::{KmallocFlags, frame_area::FRAME_SIZE, kfree, kmalloc}, serial_println};

fn assert_aligned(ptr: usize) {
    assert!(
        ptr & (FRAME_SIZE - 1) == 0,
        "Pointer {:#x} is not page-aligned",
        ptr
    );
}

fn range_overlap(a: usize, a_size: usize, b: usize, b_size: usize) -> bool {
    let a_end = a + a_size;
    let b_end = b + b_size;
    !(a_end <= b || b_end <= a)
}

pub fn test_pmm_basic() {
    serial_println!("[PMM TEST] basic alloc/free");

    let mut ptrs = [0usize; 16];

    for i in 0..ptrs.len() {
        let p = kmalloc(FRAME_SIZE, KmallocFlags::Kernel | KmallocFlags::Zeroed);
        assert!(p != 0, "kmalloc returned null");
        assert_aligned(p);
        ptrs[i] = p;
    }

    for &p in &ptrs {
        kfree(p);
    }

    serial_println!("[PMM TEST] basic alloc/free OK");
}

pub fn test_pmm_zeroed() {
    serial_println!("[PMM TEST] zeroed memory");

    let p = kmalloc(FRAME_SIZE, KmallocFlags::Kernel | KmallocFlags::Zeroed);
    assert!(p != 0);

    let slice = unsafe {
        core::slice::from_raw_parts(p as *const u8, FRAME_SIZE)
    };

    for (i, &b) in slice.iter().enumerate() {
        assert!(b == 0, "byte {} not zeroed", i);
    }

    kfree(p);

    serial_println!("[PMM TEST] zeroed memory OK");
}

pub fn test_pmm_buddy_merge() {
    serial_println!("[PMM TEST] buddy merge");

    let p1 = kmalloc(2 * FRAME_SIZE, KmallocFlags::Kernel);
    let p2 = kmalloc(2 * FRAME_SIZE, KmallocFlags::Kernel);

    assert!(p1 != 0 && p2 != 0);
    assert_aligned(p1);
    assert_aligned(p2);

    kfree(p2);
    kfree(p1);

    let p3 = kmalloc(4 * FRAME_SIZE, KmallocFlags::Kernel);
    assert!(p3 != 0);
    assert_aligned(p3);

    kfree(p3);

    serial_println!("[PMM TEST] buddy merge OK");
}

pub fn test_pmm_no_overlap() {
    serial_println!("[PMM TEST] no overlap");

    const N: usize = 8;
    let mut ptrs = [0usize; N];
    let mut sizes = [0usize; N];

    for i in 0..N {
        let size = (i + 1) * FRAME_SIZE;
        let p = kmalloc(size, KmallocFlags::Kernel);
        assert!(p != 0);

        ptrs[i] = p;
        sizes[i] = size;
    }

    for i in 0..N {
        for j in (i + 1)..N {
            assert!(
                !range_overlap(ptrs[i], sizes[i], ptrs[j], sizes[j]),
                "overlap between allocations {} and {}",
                i, j
            );
        }
    }

    for &p in &ptrs {
        kfree(p);
    }

    serial_println!("[PMM TEST] no overlap OK");
}

pub fn test_pmm_stress() {
    serial_println!("[PMM TEST] stress");

    const N: usize = 64;
    let mut ptrs = [0usize; N];

    for round in 0..10 {
        for i in 0..N {
            let size = ((i % 4) + 1) * FRAME_SIZE;
            ptrs[i] = kmalloc(size, KmallocFlags::Kernel);
            assert!(ptrs[i] != 0);
        }

        for i in (0..N).step_by(2) {
            kfree(ptrs[i]);
        }
        for i in (1..N).step_by(2) {
            kfree(ptrs[i]);
        }

        serial_println!("[PMM TEST] round {} OK", round);
    }

    serial_println!("[PMM TEST] stress OK");
}

pub fn test_pmm_all() {
    serial_println!("================ PMM TESTS ================");

    test_pmm_basic();
    test_pmm_zeroed();
    test_pmm_buddy_merge();
    test_pmm_no_overlap();
    test_pmm_stress();

    serial_println!("============= PMM TESTS PASSED =============");
}

