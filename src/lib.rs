//! Writes panic messages to the beginning of RAM
//!
//! This crate contains an implementation of `panic_fmt` that logs panic messages to the beginning
//! of RAM, recklessly overwriting the previous contents of that area. After logging the message
//! the panic handler goes into an infinite loop, so a debugging probe can connect and pick up the
//! panic.
//!
//! Unlike other methods this allows to discover the panic reason post-mortem by attaching a
//! debugger probe after the device crashed.
//!
//! Currently this crate was only tested on ARM Cortex-M architecture but should be easily portable
//! to other platforms as required.
//!
//! # Usage
//!
//! ``` ignore
//! #![no_std]
//!
//! extern crate panic_ramdump;
//!
//! fn main() {
//!     panic!("FOO")
//! }
//! ```
//!
//! ``` text
//! (gdb) x/s 0x20000000
//! 0x20000000:	"panicked at 'FOO!', src/main.rs:6:5\n\276\244\001"
//! ```
//!
#![allow(clippy::empty_loop)]
#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]

use core::fmt::Write;
use core::panic::PanicInfo;

use cortex_m::interrupt;

struct Ram {
    offset: u32,
}

/// Internal Write implementation to output the formatted panic string into RAM
impl core::fmt::Write for Ram {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        // Obtain RAM start address from linker symbol _sbss
        extern "C" {
            static mut __sbss: u8;
        }

        let data = s.as_bytes();
        let len = data.len();

        unsafe {
            core::ptr::copy(
                data.as_ptr() as *mut u8,
                (&mut __sbss as *mut u8).offset(self.offset as isize),
                len,
            )
        };

        self.offset += len as u32;
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    interrupt::disable();

    writeln!(Ram { offset: 0 }, "{}", info).ok();

    loop {}
}
