use linked_list_allocator::LockedHeap;

#[global_allocator]
pub static ALLOCATOR: linked_list_allocator::LockedHeap = LockedHeap::empty();

fn init_kernel_heap(vbase: *mut u8, size: usize) {
    unsafe {
        ALLOCATOR.lock().init(
            vbase,
            size as usize
        );
    }
}