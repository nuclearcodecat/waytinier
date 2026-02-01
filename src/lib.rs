#![feature(unix_socket_ancillary_data)]
#![feature(variant_count)]
#![feature(deque_extend_front)]
#![feature(random)]

use std::sync::OnceLock;

pub mod abstraction;
pub(crate) mod linux;
pub(crate) mod wayland;

// todo
// - use OwnedFd because it's stupid to use OwnedFd

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
//
// there's lots of notes about dmabuf in wayland/dmabuf.rs and there
// probably will be more

pub const NONE: &str = "\x1b[0m";
pub const RED: &str = "\x1b[31m";
pub const CYAN: &str = "\x1b[36m";
pub const YELLOW: &str = "\x1b[33m";
pub const GREEN: &str = "\x1b[32m";
pub const WHITE: &str = "\x1b[37m";
pub const PURPLE: &str = "\x1b[35m";

pub(crate) static DEBUGLVL: OnceLock<isize> = OnceLock::new();

pub fn init_logger() {
	let dbug: isize =
		std::env::var("WAYTINIER_DEBUGLVL").unwrap_or(String::from("2")).parse().unwrap_or(2);
	let _ = DEBUGLVL.set(dbug);
}

#[cfg(not(feature = "nolog"))]
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
	Verbose,
	SuperVerbose,
}

#[macro_export]
macro_rules! wlog {
	($lvl:expr, $header:expr, $msg:expr, $header_color:expr, $msg_color:expr) => {{
		#[cfg(not(feature = "nolog"))]
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
		#[cfg(feature = "nolog")]
		let _ = (&$lvl, &$header, &$msg, &$header_color, &$msg_color);
	}};
}

#[macro_export]
macro_rules! dbug {
	($msg:expr) => {
		$crate::wlog!($crate::DebugLevel::Important, "DEBUG", $msg, $crate::CYAN, $crate::CYAN);
	};
}

#[macro_export]
macro_rules! make_drop_impl {
	($kind:ty, $method:ident) => {
		impl Drop for $kind {
			fn drop(&mut self) {
				$crate::wlog!(
					$crate::DebugLevel::Important,
					self.kind_as_str(),
					"dropping self",
					$crate::WHITE,
					$crate::CYAN
				);
				if let Err(er) = self.queue_request(self.$method()) {
					// god is dead
					$crate::wlog!(
						$crate::DebugLevel::Error,
						self.kind_as_str(),
						format!("queuing damnation failed: {er}"),
						$crate::WHITE,
						$crate::RED
					);
				} else {
					$crate::wlog!(
						$crate::DebugLevel::Important,
						self.kind_as_str(),
						"damnation queued",
						$crate::WHITE,
						$crate::CYAN
					);
				}
			}
		}
	};
}
