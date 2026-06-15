#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::yield_;

#[unsafe(no_mangle)]
fn main() -> i32 {
    println!("[MyApp] Hello, this is my own application!");
    for i in 1..=3 {
        println!("[MyApp] Working... step {}", i);
        yield_();
    }
    println!("[MyApp] Work done, exiting.");
    0
}
