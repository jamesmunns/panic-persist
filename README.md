# `panic-persist`

Writes panic messages to a section of RAM

This crate contains an implementation of `panic_fmt` that logs panic messages to a region of
RAM defined by the user, so that these messages can be retrieved on next boot, and handled
outside of panic context, by sending to a logging interface, writing to flash, etc.

Unlike other methods this allows to discover the panic reason post-mortem using normal program
control flow.

Currently this crate was only tested on ARM Cortex-M architecture but should be easily portable
to other platforms as required.

## [Documentation](https://docs.rs/panic-persist)

## Usage

### Add a section to your linker script

You will need to reserve a section of RAM to be used to persist messages. This section must be
large enough to hold the 8 byte header, as well as any panic messages you would like to persist.
If there is not suitable space in the section, the panic message will be truncated.

This section should be outside of any other sections, to prevent program initialization from
zeroing or otherwise modifying these sections on boot.

`memory.x` file before modification:

``` ignore
MEMORY
{
  /* NOTE K = KiBi = 1024 bytes */
  FLASH : ORIGIN  = 0x00000000, LENGTH = 512K
  RAM : ORIGIN    = 0x20000000, LENGTH = 64K
}
```

`memory.x` file after modification to hold a 1K region:

``` ignore
MEMORY
{
  /* NOTE K = KiBi = 1024 bytes */
  FLASH : ORIGIN  = 0x00000000, LENGTH = 512K
  RAM : ORIGIN    = 0x20000000, LENGTH = 63K
  PANDUMP: ORIGIN = 0x2000FC00, LENGTH = 1K
}

_panic_dump_start = ORIGIN(PANDUMP);
_panic_dump_end   = ORIGIN(PANDUMP) + LENGTH(PANDUMP);
```


### Program Usage Example

``` ignore
#![no_std]

use panic_persist as _;

#[entry]
fn main() -> ! {
    // Normal board setup...

    // Check if there was a panic message, if so, send to UART
    if let Some(msg) = get_panic_message_bytes() {
        board.uart.write(msg);
    }

    // ...
}
```

## Features

There is one optional feature, `utf8`. This allows the panic message to be returned
as a `&str` rather than `&[u8]`, for easier printing. As this requires the ability
to validate the UTF-8 string (to ensure it wasn't truncated mid-character), it may
increase code size usage, and is by default off.

## Provenance

This crate was inspired (and forked from) the [`panic-ramdump`] crate.

[`panic-ramdump`](https://github.com/therealprof/panic-ramdump)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
