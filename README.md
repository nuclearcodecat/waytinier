waytinier is a tiny experimental wayland client library for rust.
it is dependant on nothing but std and libc (which std also depends on).
i will probably definitely absolutely 99.9%ly not release it on crates

waytinier is tiny by design. the _machine_ example can open a window and draw an image while weighing a bit over 650KiB (with default cargo/rustc settings)
with these cargo optimizations for size enabled, i managed to get the binary size down to ~380KiB:
 - elf stripping enabled
 - opt level set to »z«
 - lto set to »fat«
 - only one codegen unit enabled
 - panic unwinding disabled
interestingly, setting the opt-level to »s« **increased** the "raw" binary's size by 4KiB and »z« increased it by **50KiB**!
these settings increased the build times from ~0.7s to ~3.6s

waytinier currently offers window (xdg_toplevel) creation and drawing on buffers.
it is bare by design. it shouldn't be hard to extend functionality though.
documentation is highly lacking, as in, there is none. i may get to that one day

the _WAYTINIER_DEBUGLEVEL_ environment variable can be set to values from 0 to 4 to change the amount of logs emmited. a _nolog_ feature is available to disable logging completely

future plans include keyboard and/or mouse support and some better examples with usage of some opengl lib that could modify the given slice

see the examples dir for a simple example.

building waytinier **requires** the nightly rust compiler toolchain.
the most important feature is *unix_socket_ancillary_data*, which allows me to attach file descriptors to wayland requests
