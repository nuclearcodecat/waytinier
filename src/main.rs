#![allow(dead_code)]
#![feature(unix_socket_ancillary_data)]

use std::{env, error::Error};

mod wayland;

use crate::wayland::{
	Buffer, Compositor, Display, IdentManager, Registry, SharedMemory, SharedMemoryPool, XdgWmBase,
	wire::{MessageManager, WireArgument},
};

fn main() -> Result<(), Box<dyn Error>> {
	let mut wlim = IdentManager::default();
	let mut wlmm = MessageManager::new(&env::var("WAYLAND_DISPLAY")?)?;

	let mut display = Display::new(&mut wlim);
	let mut registry = Registry::new_filled(&mut display, &mut wlmm, &mut wlim)?;
	let compositor = Compositor::new_bound(&mut registry, &mut wlmm, &mut wlim)?;
	let mut surface = compositor.make_surface(&mut wlmm, &mut wlim)?;
	let mut shm =
		SharedMemory::new_bound_initialized(&mut display, &mut registry, &mut wlmm, &mut wlim)?;
	let mut shm_pool =
		SharedMemoryPool::new_initialized(&mut shm, 500 * 500 * 4, &mut wlmm, &mut wlim)?;
	let buf = Buffer::new_initialized(
		&mut shm_pool,
		(0, 500, 500, 500),
		wayland::PixelFormat::Xrgb888,
		&mut wlmm,
		&mut wlim,
	)?;
	let xdg_wm_base = XdgWmBase::new_bound(&mut display, &mut registry, &mut wlmm, &mut wlim)?;
	let xdg_surface =
		xdg_wm_base.make_xdg_surface_from_wl_surface(surface.id, &mut wlmm, &mut wlim)?;
	let xdg_toplevel = xdg_surface.make_xdg_toplevel(&mut wlmm, &mut wlim)?;
	surface.attach_buffer(buf.id, &mut wlmm)?;
	surface.commmit(&mut wlmm)?;
	println!("hello");

	display.wl_sync(&mut wlmm, &mut wlim)?;

	// wait for ping
	let mut ponged = false;
	while !ponged {
		wlmm.get_events(&mut wlim)?;
		while let Some(ev) = wlmm.q.pop_front() {
			if ev.recv_id == xdg_wm_base.id
				&& ev.opcode == 0
				&& let WireArgument::UnInt(serial) = ev.args[0]
			{
				xdg_wm_base.wl_pong(&mut wlmm, serial)?;
				ponged = true;
				break;
			} else {
				println!("{:#?}", ev);
			}
		}
	}

	// USE INTERMUT SO SHIT DROPS WHEN PANICKING
	xdg_wm_base.destroy(&mut wlmm, &mut wlim)?;
	buf.destroy(&mut wlmm, &mut wlim)?;
	shm_pool.destroy(&mut wlmm, &mut wlim)?;
	Ok(())
}
