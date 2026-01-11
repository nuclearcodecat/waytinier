use std::{cell::RefCell, env, error::Error, rc::Rc};

use wayland_raw::wayland::{
	Context, IdentManager,
	compositor::Compositor,
	display::Display,
	shm::{PixelFormat, SharedMemory},
	wire::MessageManager, xdgshell::XdgWmBase,
};

fn main() -> Result<(), Box<dyn Error>> {
	let wlim = IdentManager::default();
	let wlmm = MessageManager::new(&env::var("WAYLAND_DISPLAY")?)?;
	let ctx = Context::new(wlmm, wlim);
	let ctx = Rc::new(RefCell::new(ctx));

	let display = Display::new(ctx.clone());
	let registry = display.borrow_mut().make_registry()?;
	let compositor = Compositor::new_bound(&mut registry.borrow_mut(), ctx.clone())?;
	let surface = compositor.borrow_mut().make_surface()?;
	let shm = SharedMemory::new_bound_initialized(&mut registry.borrow_mut(), ctx.clone())?;
	let shm_pool = shm.borrow_mut().make_pool(500 * 500 * 4)?;
	let buf = shm_pool.borrow_mut().make_buffer((0, 500, 500, 500), PixelFormat::Xrgb888)?;
	let xdg_wm_base = XdgWmBase::new_bound(&mut registry.borrow_mut())?;
	let xdg_surface = xdg_wm_base.borrow_mut().make_xdg_surface(surface.borrow().id)?;
	let xdg_toplevel = xdg_surface.borrow_mut().make_xdg_toplevel()?;
	surface.borrow_mut().attach_buffer(buf.borrow().id)?;
	surface.borrow_mut().commit()?;
	println!("hello");

	display.borrow_mut().sync()?;

	// USE INTERMUT SO SHIT DROPS WHEN PANICKING
	xdg_wm_base.borrow_mut().destroy()?;
	buf.borrow_mut().destroy()?;
	shm_pool.borrow_mut().destroy()?;
	Ok(())
}
