#![allow(dead_code)]

use std::{env, error::Error};

mod wayland;

use crate::wayland::{Display, IdManager, Registry, WaylandObject, wire::MessageManager};

fn main() -> Result<(), Box<dyn Error>> {
	let display_name = env::var("WAYLAND_DISPLAY")?;
	let mut wlim = IdManager::default();
	let mut wlmm = MessageManager::new(&display_name)?;
	let mut display = Display::new(&mut wlim);
	let reg_id = display.wl_get_registry(&mut wlmm, &mut wlim)?;
	let mut registry = Registry::new(reg_id);

	let get_registry_callback_id = display.wl_sync(&mut wlmm, &mut wlim)?;

	let mut read = wlmm.get_events()?;
	while read.is_none() {
		read = wlmm.get_events()?;
	}
	let read = &read.unwrap();
	registry.fill(read)?;
	println!("==== REGISTRY\n{:#?}", registry.inner);

	registry.wl_bind(&mut wlmm, &mut wlim, WaylandObject::Compositor)?;

	wlmm.discon()?;
	println!("good");
	Ok(())
}
