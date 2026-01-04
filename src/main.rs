#![allow(dead_code)]

use std::{env, error::Error};

mod wayland;

use crate::wayland::{
	Buffer, Compositor, Display, IdManager, Registry, SharedMemory, SharedMemoryPool,
	wire::MessageManager,
};

fn main() -> Result<(), Box<dyn Error>> {
	let mut wlim = IdManager::default();
	let mut wlmm = MessageManager::new(&env::var("WAYLAND_DISPLAY")?)?;

	let mut display = Display::new(&mut wlim);
	let mut registry = Registry::new_bound_filled(&mut display, &mut wlmm, &mut wlim)?;
	let compositor = Compositor::new_bound(&mut registry, &mut wlmm, &mut wlim)?;
	// let mut shm = SharedMemory::new_bound(&mut registry, &mut wlmm, &mut wlim)?;
	// let mut shm_pool = SharedMemoryPool::new_bound(&mut shm, 500 * 500 * 4, &mut wlmm, &mut wlim)?;
	// let buf = Buffer::new_initialized(&mut shm_pool, (0, 500, 500, 500), wayland::PixelFormat::Xrgb888, &mut wlmm, &mut wlim)?;

	// errors should fire anyway but i need to specify something
	wlmm.get_events_blocking(0, wayland::WaylandObjectKind::Display)?;

	// shm_pool.destroy(&mut wlmm, &mut wlim)?;
	Ok(())
}
