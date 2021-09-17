//! # `panic-persist`
//!
//! Writes panic messages to a section of RAM
//!
//! This crate contains an implementation of `panic_fmt` that logs panic messages to a region of
//! RAM defined by the user, so that these messages can be retrieved on next boot, and handled
//! outside of panic context, by sending to a logging interface, writing to flash, etc.
//!
//! After logging the message to RAM, the device will be soft-reset automatically.
//!
//! Unlike other methods this allows to discover the panic reason post-mortem using normal program
//! control flow.
//!
//! Currently this crate was only tested on ARM Cortex-M architecture but should be easily portable
//! to other platforms as required.
//!
//! ## Usage
//!
//! ### Add a section to your linker script
//!
//! You will need to reserve a section of RAM to be used to persist messages. This section must be
//! large enough to hold the 8 byte header, as well as any panic messages you would like to persist.
//! If there is not suitable space in the section, the panic message will be truncated.
//!
//! This section should be outside of any other sections, to prevent program initialization from
//! zeroing or otherwise modifying these sections on boot.
//!
//! `memory.x` file before modification:
//!
//! ``` ignore
//! MEMORY
//! {
//!   /* NOTE K = KiBi = 1024 bytes */
//!   FLASH : ORIGIN  = 0x00000000, LENGTH = 512K
//!   RAM : ORIGIN    = 0x20000000, LENGTH = 64K
//! }
//! ```
//!
//! `memory.x` file after modification to hold a 1K region:
//!
//! ``` ignore
//! MEMORY
//! {
//!   /* NOTE K = KiBi = 1024 bytes */
//!   FLASH : ORIGIN  = 0x00000000, LENGTH = 512K
//!   RAM : ORIGIN    = 0x20000000, LENGTH = 63K
//!   PANDUMP: ORIGIN = 0x2000FC00, LENGTH = 1K
//! }
//!
//! _panic_dump_start = ORIGIN(PANDUMP);
//! _panic_dump_end   = ORIGIN(PANDUMP) + LENGTH(PANDUMP);
//! ```
//!
//!
//! ### Program Usage Example
//!
//! ``` ignore
//! #![no_std]
//!
//! use panic_persist as _;
//!
//! #[entry]
//! fn main() -> ! {
//!     // Normal board setup...
//!
//!     // Check if there was a panic message, if so, send to UART
//!     if let Some(msg) = get_panic_message_bytes() {
//!         board.uart.write(msg);
//!     }
//!
//!     // ...
//! }
//! ```
//!
//! ## Features
//!
//! There are two optional features, `utf8` and `custom-panic-handler`.
//!
//! ### utf8
//!
//! This allows the panic message to be returned
//! as a `&str` rather than `&[u8]`, for easier printing. As this requires the ability
//! to validate the UTF-8 string (to ensure it wasn't truncated mid-character), it may
//! increase code size usage, and is by default off.
//!
//! ### custom-panic-handler
//!
//! This disables the panic handler from this library so that any user can implement their own.
//! To persist panic messages, the function `report_panic_info` is made available;
//!
//! ```rust
//! // My custom panic implementation
//! #[panic_handler]
//! fn panic(info: &PanicInfo) -> ! {
//!     // ...
//!     panic_persist::report_panic_info(info);
//!     // ...
//! }
//! ```

#![allow(clippy::empty_loop)]
#![deny(missing_docs)]
#![deny(warnings)]
#![no_std]

use core::cmp::min;
use core::fmt::Write;
use core::mem::size_of;
use core::panic::PanicInfo;

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

/// Report the panic so the message is persisted.
///
/// This function is used in custom panic handlers.
#[cfg(feature = "custom-panic-handler")]
pub fn report_panic_info(info: &PanicInfo) {
    writeln!(Ram { offset: 0 }, "{}", info).ok();
}

#[cfg(not(feature = "custom-panic-handler"))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cortex_m::interrupt::disable();

    #[cfg(feature = "min-panic")]
    if let Some(location) = info.location() {
        writeln!(Ram { offset: 0 }, "Panicked at {}", location).ok();
    } else {
        writeln!(Ram { offset: 0 }, "Panic occured!").ok();
    }

    #[cfg(not(feature = "min-panic"))]
    writeln!(Ram { offset: 0 }, "{}", info).ok();

    cortex_m::peripheral::SCB::sys_reset();
}
