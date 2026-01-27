#![feature(unix_socket_ancillary_data)]
#![feature(variant_count)]
#![feature(deque_extend_front)]
#![feature(random)]

use std::sync::OnceLock;

pub mod abstraction;
pub mod wayland;

// ===== future me notes
//
// call handle_events() on Context to collect events from the pipe
// which will call handle() on the Wlto's (wayland trait objects).
// they get the opcode and return EventActions, like log messages,
// returning requests (ping, pong) and other stuff
//
// to make a window, spawn an XdgTopLevel through XdgWmBase (this
// base also gets hooked to the Context). then make a Buffer and
// attach it to a Surface with attach_buffer_obj(). you also need
// a SharedMemory(Pool). spawn a loop and do whatever.
// to get smooth frames, use a frame() callback on the surface.
// Callback has a done attribute and XdgWmBase has an is_configured
// attribute.
// to draw on the buffer, get the slice (it's an attrib) from
// SharedMemoryPool. then attach the buffer to the surface (wayland
// call, not internal function) and commit it. the buffer needs to
// be damaged to see any changes. repaint() damages the whole
// buffer.
//
// if init_logger() is not called in the bin, the debug level will
// always be 0 (none)
//
// DO NOT ATTACH A BUFFER BEFORE GETTING A CORRECT SIZE FROM THE
// COMPOSITOR, YOU WASTED HOURS

pub const NONE: &str = "\x1b[0m";
pub const RED: &str = "\x1b[31m";
pub const CYAN: &str = "\x1b[36m";
pub const YELLOW: &str = "\x1b[33m";
pub const GREEN: &str = "\x1b[32m";
pub const WHITE: &str = "\x1b[37m";
pub const PURPLE: &str = "\x1b[35m";

pub(crate) static DEBUGLVL: OnceLock<isize> = OnceLock::new();

pub fn init_logger() {
	let dbug: isize = std::env::var("DEBUGLVL").unwrap_or(String::from("0")).parse().unwrap_or(0);
	let _ = DEBUGLVL.set(dbug);
}

pub(crate) fn get_dbug() -> isize {
	*DEBUGLVL.get().unwrap_or(&0)
}

#[allow(dead_code)]
#[repr(isize)]
#[derive(PartialEq)]
pub(crate) enum DebugLevel {
	None,
	Error,
	Important,
	Trivial,
}

#[macro_export]
macro_rules! wlog {
	($lvl:expr, $header:expr, $msg:expr, $header_color:expr, $msg_color:expr) => {
		if $crate::get_dbug() >= $lvl as isize {
			println!(
				"{}\x1b[7m! {} !\x1b[0m{} {}{}{}",
				$header_color,
				$header,
				$crate::NONE,
				$msg_color,
				$msg,
				$crate::NONE,
			)
		}
	};
}

#[macro_export]
macro_rules! dbug {
	($msg:expr) => {
		$crate::wlog!($crate::DebugLevel::Important, "DEBUG", $msg, $crate::CYAN, $crate::CYAN);
	};
}
