mod heap_allocator;
mod address;
mod frame_allocator;
mod page_table;
mod memory_set;

pub use address::{PhysAddr, VirtAddr, PhysPageNum, VirtPageNum, VPNRange, StepByOne};
pub use frame_allocator::{FrameTracker, frame_alloc};
pub use page_table::{PageTable, PageTableEntry, PTEFlags, translated_byte_buffer};
pub use memory_set::{MemorySet, KERNEL_SPACE, MapPermission};
pub use memory_set::remap_test;

pub fn init() {
    heap_allocator::init_heap();
    heap_allocator::heap_test();
    frame_allocator::init_frame_allocator();
    frame_allocator::frame_allocator_test();
    KERNEL_SPACE.exclusive_access().activate();
    remap_test();
}
