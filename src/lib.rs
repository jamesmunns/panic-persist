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

use core::cmp::min;
use core::fmt::Write;
use core::mem::size_of;
use core::panic::PanicInfo;

use cortex_m::interrupt;

struct Ram {
    offset: usize,
}

/// Internal Write implementation to output the formatted panic string into RAM
impl core::fmt::Write for Ram {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        // Obtain panic region start and end from linker symbol _panic_dump_start and _panic_dump_end
        extern "C" {
            static mut _panic_dump_start: u8;
            static mut _panic_dump_end: u8;
        }

        // Get the data about the string that is being written now
        let data = s.as_bytes();
        let len = data.len();

        // Obtain info about the panic dump region
        let start_ptr = unsafe { &mut _panic_dump_start as *mut u8 };
        let end_ptr = unsafe { &mut _panic_dump_end as *mut u8 };
        let max_len = end_ptr as usize - start_ptr as usize;
        let max_len_str = max_len - size_of::<usize>() - size_of::<usize>();

        // If we have written the full length of the region, we can't write any
        // more. This could happen with multiple writes with this implementation
        if self.offset >= max_len_str {
            return Ok(());
        }

        // We should write the size of the string, or the amount of space
        // we have remaining, whichever is less
        let str_len = min(max_len_str - self.offset, len);

        unsafe {
            // Write the magic word for later detection
            start_ptr.cast::<usize>().write_unaligned(0x0FACADE0);

            // For now, skip writing the length...

            // Write the string to RAM
            core::ptr::copy(
                data.as_ptr() as *mut u8,
                start_ptr.offset(8).offset(self.offset as isize),
                str_len,
            );

            // Increment the offset so later writes will be appended
            self.offset += str_len;

            // ... and now write the current offset (or total size) to the size location
            start_ptr
                .offset(4)
                .cast::<usize>()
                .write_unaligned(self.offset);
        };

        Ok(())
    }
}

/// Get the panic message from the last boot, if any.
/// This method may possibly not return valid UTF-8 if the message
/// was truncated before the end of a full UTF-8 character. Care must
/// be taken before treating this as a proper &str.
///
/// If a message existed, this function will only return the value once
/// (subsequent calls will return None)
pub fn get_panic_message_bytes() -> Option<&'static [u8]> {
    // Obtain panic region start and end from linker symbol _panic_dump_start and _panic_dump_end
    extern "C" {
        static mut _panic_dump_start: u8;
        static mut _panic_dump_end: u8;
    }

    let start_ptr = unsafe { &mut _panic_dump_start as *mut u8 };

    if 0x0FACADE0 != unsafe { core::ptr::read_unaligned(start_ptr.cast::<usize>()) } {
        return None;
    }

    // Clear the magic word to prevent this message from "sticking"
    // across multiple boots
    unsafe {
        start_ptr.cast::<usize>().write_unaligned(0x00000000);
    }

    // Obtain info about the panic dump region
    let end_ptr = unsafe { &mut _panic_dump_end as *mut u8 };
    let max_len = end_ptr as usize - start_ptr as usize;
    let max_len_str = max_len - size_of::<usize>() - size_of::<usize>();

    let len = unsafe { core::ptr::read_unaligned(start_ptr.offset(4).cast::<usize>()) };

    if len > max_len_str {
        return None;
    }

    // TODO: This is prooooooooobably undefined behavior
    let byte_slice = unsafe { core::slice::from_raw_parts(start_ptr.offset(8), len) };

    Some(byte_slice)
}

/// Get the panic message from the last boot, if any. If any invalid
/// UTF-8 characters occur, the message will be truncated before the
/// first error.
///
/// If a message existed, this function will only return the value once
/// (subsequent calls will return None)
#[cfg(feature = "utf8")]
pub fn get_panic_message_utf8() -> Option<&'static str> {
    let bytes = get_panic_message_bytes()?;

    use core::str::from_utf8;

    match from_utf8(bytes) {
        Ok(stir) => Some(stir),
        Err(utf_err) => {
            match from_utf8(&bytes[..utf_err.valid_up_to()]) {
                Ok(stir) => Some(stir),
                Err(_) => {
                    // This shouldn't be possible...
                    None
                }
            }
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    interrupt::disable();

    writeln!(Ram { offset: 0 }, "{}", info).ok();

    cortex_m::peripheral::SCB::sys_reset();
}
