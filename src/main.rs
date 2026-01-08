#![allow(dead_code)]
#![feature(unix_socket_ancillary_data)]

use std::{env, error::Error, time::Duration};

mod wayland;

use crate::wayland::{
	Buffer, Compositor, Display, IdentManager, Registry, SharedMemory, SharedMemoryPool, XdgWmBase,
	wire::MessageManager,
};

fn main() -> Result<(), Box<dyn Error>> {
	let mut wlim = IdentManager::default();
	let mut wlmm = MessageManager::new(&env::var("WAYLAND_DISPLAY")?)?;

	let mut display = Display::new(&mut wlim);
	let mut registry = Registry::new_bound_filled(&mut display, &mut wlmm, &mut wlim)?;
	let reg_syncid = display.wl_sync(&mut wlmm, &mut wlim)?;

	let compositor = Compositor::new_bound(&mut registry, &mut wlmm, &mut wlim)?;
	let mut shm = SharedMemory::new_bound(&mut registry, &mut wlmm, &mut wlim)?;
	let mut shm_pool =
		SharedMemoryPool::new_initialized(&mut shm, 500 * 500 * 4, &mut wlmm, &mut wlim)?;
	let buf = Buffer::new_initialized(
		&mut shm_pool,
		(0, 500, 500, 500),
		wayland::PixelFormat::Xrgb888,
		&mut wlmm,
		&mut wlim,
	)?;
	let xdg_wm_base = XdgWmBase::new_bound(&mut registry, &mut wlmm, &mut wlim)?;

	std::thread::sleep(Duration::from_millis(100));
	// errors should fire anyway but i need to specify something
	// maybe a seperate get_errors method?
	// wlmm.get_events_blocking(0, wayland::WaylandObjectKind::Display)?;

	// let mut done = false;
	// while !done {
	// 	if let Some(msg) = wlmm.get_events(xdg_wm_base.id, &wayland::WaylandObjectKind::XdgWmBase)? {
	// 		todo!()
	// 	};
	// 	done = true;
	// }
	//
	// USE INTERMUT SO SHIT DROPS WHEN PANICKING
	shm_pool.destroy(&mut wlmm, &mut wlim)?;
	buf.destroy(&mut wlmm, &mut wlim)?;
	xdg_wm_base.destroy(&mut wlmm, &mut wlim)?;
	Ok(())
}
