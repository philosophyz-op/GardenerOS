use core::panic::PanicInfo;
use crate::{println, sbi::shutdown};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("Panicked: {:?}", info);
    shutdown()
}
