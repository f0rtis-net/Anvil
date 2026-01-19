#[cfg(feature = "pmm_tests")]
pub mod pmm_tests {
    use x86_64::VirtAddr;

    use crate::{arch::amd64::memory::pmm::{physical_alloc::{KmallocFlags, kfree, kmalloc}, sparsemem::PAGE_SIZE}, serial_println};

    fn assert_zeroed(ptr: usize, size: usize) {
        unsafe {
            let slice = core::slice::from_raw_parts(ptr as *const u8, size);
            for (i, b) in slice.iter().enumerate() {
                assert!(
                    *b == 0,
                    "Memory not zeroed at offset {} (value={:#x})",
                    i,
                    b
                );
            }
        }
    }

    fn fill(ptr: usize, size: usize, val: u8) {
        unsafe {
            core::ptr::write_bytes(ptr as *mut u8, val, size);
        }
    }

    fn check(ptr: usize, size: usize, val: u8) {
        unsafe {
            let slice = core::slice::from_raw_parts(ptr as *const u8, size);
            for (i, b) in slice.iter().enumerate() {
                assert!(
                    *b == val,
                    "Memory corrupted at offset {} (expected={:#x}, got={:#x})",
                    i,
                    val,
                    b
                );
            }
        }
    }

    pub fn run_all() {
        serial_println!("\n========== PMM TESTS START ==========");

        test_basic_alloc();
        test_zeroed_alloc();
        test_small_alloc();
        test_page_alloc();
        test_multiple_allocs();
        test_free_and_reuse();
        test_various_sizes();
        test_stress();
        test_no_overlap();

        serial_println!("========== PMM TESTS PASSED ==========\n");
    }

    fn test_basic_alloc() {
        let p = kmalloc(64, KmallocFlags::Zeroed)
            .expect("basic alloc failed");
        fill(p.as_u64() as usize, 64, 0xAA);
        check(p.as_u64() as usize, 64, 0xAA);
        kfree(p);
        serial_println!("test_basic_alloc OK");
    }

    fn test_zeroed_alloc() {
        let p = kmalloc(128, KmallocFlags::Zeroed)
            .expect("zeroed alloc failed");
        assert_zeroed(p.as_u64() as usize, 128);
        kfree(p);
        serial_println!("test_zeroed_alloc OK");
    }

    fn test_small_alloc() {
        let p = kmalloc(15, KmallocFlags::Zeroed)
            .expect("small alloc failed");
        fill(p.as_u64() as usize, 15, 0x11);
        check(p.as_u64() as usize, 15, 0x11);
        kfree(p);
        serial_println!("test_small_alloc OK");
    }

    fn test_page_alloc() {
        let p = kmalloc(PAGE_SIZE, KmallocFlags::Zeroed)
            .expect("page alloc failed");
        assert_zeroed(p.as_u64() as usize, PAGE_SIZE);
        kfree(p);
        serial_println!("test_page_alloc OK");
    }

    fn test_multiple_allocs() {
        let a = kmalloc(64, KmallocFlags::Zeroed).unwrap();
        let b = kmalloc(128, KmallocFlags::Zeroed).unwrap();
        let c = kmalloc(256, KmallocFlags::Zeroed).unwrap();

        fill(a.as_u64() as usize, 64, 0xA1);
        fill(b.as_u64() as usize, 128, 0xB2);
        fill(c.as_u64() as usize, 256, 0xC3);

        check(a.as_u64() as usize, 64, 0xA1);
        check(b.as_u64() as usize, 128, 0xB2);
        check(c.as_u64() as usize, 256, 0xC3);

        kfree(a);
        kfree(b);
        kfree(c);

        serial_println!("test_multiple_allocs OK");
    }

    fn test_free_and_reuse() {
        let p1 = kmalloc(128, KmallocFlags::Zeroed).unwrap();
        fill(p1.as_u64() as usize, 128, 0x77);
        kfree(p1);

        let p2 = kmalloc(128, KmallocFlags::Zeroed).unwrap();
        fill(p2.as_u64() as usize, 128, 0x88);
        check(p2.as_u64() as usize, 128, 0x88);
        kfree(p2);

        serial_println!("test_free_and_reuse OK");
    }

    fn test_various_sizes() {
        for size in [
            1, 7, 15, 31, 63, 127,
            256, 511, 1024, 2048,
            PAGE_SIZE - 1,
            PAGE_SIZE,
            PAGE_SIZE + 1,
        ] {
            let p = kmalloc(size, KmallocFlags::Zeroed)
                .unwrap_or_else(|| panic!("alloc failed for size {}", size));
            assert_zeroed(p.as_u64() as usize, size);
            kfree(p);
        }

        serial_println!("test_various_sizes OK");
    }

    fn test_stress() {
        const N: usize = 256;
        let mut ptrs = [VirtAddr::zero(); N];

        for i in 0..N {
            ptrs[i] = kmalloc(64, KmallocFlags::Zeroed)
                .unwrap_or_else(|| panic!("stress alloc failed at {}", i));
            fill(ptrs[i].as_u64() as usize, 64, i as u8);
        }

        for i in 0..N {
            check(ptrs[i].as_u64() as usize, 64, i as u8);
        }

        for i in 0..N {
            kfree(ptrs[i]);
        }

        serial_println!("test_stress OK");
    }

    fn test_no_overlap() {
        let a = kmalloc(128, KmallocFlags::Zeroed).unwrap();
        let b = kmalloc(128, KmallocFlags::Zeroed).unwrap();

        assert!(
            a + 128 <= b || b + 128 <= a,
            "Memory overlap detected: a={:#x}, b={:#x}",
            a, b
        );

        kfree(a);
        kfree(b);

        serial_println!("test_no_overlap OK");
    }
}
