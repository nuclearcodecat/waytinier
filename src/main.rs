// TODO
// - read up on Weak because i think i'm doing Rc stuff wrong

use std::{cell::RefCell, env, error::Error, rc::Rc};

use wayland_raw::wayland::{
	Context, IdentManager, RcCell,
	buffer::Buffer,
	callback::Callback,
	compositor::Compositor,
	display::Display,
	shm::{PixelFormat, SharedMemory},
	wire::MessageManager,
	xdgshell::{XdgTopLevel, XdgWmBase},
};

fn main() -> Result<(), Box<dyn Error>> {
	const W: i32 = 500;
	const H: i32 = 900;

	let wlim = IdentManager::default();
	let wlmm = MessageManager::new(&env::var("WAYLAND_DISPLAY")?)?;
	let ctx = Context::new(wlmm, wlim);
	let ctx = Rc::new(RefCell::new(ctx));

	let display = Display::new(ctx.clone());
	let registry = display.borrow_mut().make_registry()?;
	ctx.borrow_mut().handle_events()?;
	let compositor = Compositor::new_bound(&mut registry.borrow_mut(), ctx.clone())?;
	let surface = compositor.borrow_mut().make_surface()?;
	let shm = SharedMemory::new_bound_initialized(&mut registry.borrow_mut(), ctx.clone())?;
	let pf = PixelFormat::Xrgb888;
	let shm_pool = shm.borrow_mut().make_pool(W * H * pf.width() as i32)?;
	ctx.borrow_mut().handle_events()?;
	let xdg_wm_base = XdgWmBase::new_bound(&mut registry.borrow_mut())?;
	let xdg_surface = xdg_wm_base.borrow_mut().make_xdg_surface(surface.clone(), (W, H))?;
	let xdg_toplevel = XdgTopLevel::new_from_xdg_surface(xdg_surface.clone(), ctx.clone())?;
	xdg_toplevel.borrow_mut().set_app_id(String::from("wayland-raw-appid"))?;
	xdg_toplevel.borrow_mut().set_title(String::from("wayland-raw-title"))?;

	let buf = Buffer::new_initalized(shm_pool.clone(), (0, W, H), pf, ctx.clone())?;
	surface.borrow_mut().attach_buffer_obj(buf.clone())?;
	surface.borrow_mut().commit()?;
	let mut frame: usize = 0;
	let mut cb: Option<RcCell<Callback>> = None;

	loop {
		ctx.borrow_mut().handle_events()?;

		if xdg_surface.borrow().is_configured {
			let ready = match &cb.clone() {
				Some(cb) => cb.borrow().done,
				None => true,
			};

			if ready {
				let new_cb = surface.borrow_mut().frame()?;
				cb = Some(new_cb);

				let (r, g, b) = hsv_to_rgb(frame as f64, 1.0, 1.0);

				unsafe {
					let slice = &mut *shm_pool.borrow_mut().slice.unwrap();
					println!("! main ! slice len: {}", slice.len());
					frame = frame.wrapping_add(1);

					for (ix, pixel) in slice.chunks_mut(4).enumerate() {
						pixel[0] = r.wrapping_add(ix as u8);
						pixel[1] = g.wrapping_add(ix as u8);
						pixel[2] = b.wrapping_add(ix as u8);
					}
				}
				surface.borrow_mut().attach_buffer()?;
				surface.borrow_mut().repaint()?;
				surface.borrow_mut().commit()?;
			}
		}
	}
}

// stolen from hsv library
pub fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (u8, u8, u8) {
	fn is_between(value: f64, min: f64, max: f64) -> bool {
		min <= value && value < max
	}

	// check_bounds(hue, saturation, value);

	let c = value * saturation;
	let h = hue / 60.0;
	let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
	let m = value - c;

	let (r, g, b): (f64, f64, f64) = if is_between(h, 0.0, 1.0) {
		(c, x, 0.0)
	} else if is_between(h, 1.0, 2.0) {
		(x, c, 0.0)
	} else if is_between(h, 2.0, 3.0) {
		(0.0, c, x)
	} else if is_between(h, 3.0, 4.0) {
		(0.0, x, c)
	} else if is_between(h, 4.0, 5.0) {
		(x, 0.0, c)
	} else {
		(c, 0.0, x)
	};

	(((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
}
