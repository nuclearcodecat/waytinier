// TODO
// - read up on Weak because i think i'm doing Rc stuff wrong

use std::{cell::RefCell, error::Error, fs::File, io::{BufRead, BufReader, Read}, rc::Rc};

use wayland_raw::wayland::{
	Context, RcCell,
	buffer::Buffer,
	callback::Callback,
	compositor::Compositor,
	display::Display,
	shm::{PixelFormat, SharedMemory},
	xdgshell::{XdgTopLevel, XdgWmBase},
};

fn main() -> Result<(), Box<dyn Error>> {
	const W: i32 = 500;
	const H: i32 = 900;

	let ctx = Rc::new(RefCell::new(Context::new_default()?));
	let display = Display::new(ctx.clone())?;
	let registry = display.borrow_mut().make_registry()?;
	ctx.borrow_mut().handle_events()?;
	let compositor = Compositor::new_bound(&mut registry.borrow_mut(), ctx.clone())?;
	let surface = compositor.borrow_mut().make_surface()?;
	let shm = SharedMemory::new_bound_initialized(&mut registry.borrow_mut(), ctx.clone())?;
	let pf = PixelFormat::Xrgb888;
	let shm_pool = shm.borrow_mut().make_pool(W * H * pf.width() as i32)?;
	ctx.borrow_mut().handle_events()?;
	let xdg_wm_base = XdgWmBase::new_bound(&mut registry.borrow_mut())?;
	let xdg_surface = xdg_wm_base.borrow_mut().make_xdg_surface(surface.clone())?;
	let xdg_toplevel = XdgTopLevel::new_from_xdg_surface(xdg_surface.clone(), ctx.clone())?;
	xdg_toplevel.borrow_mut().set_app_id(String::from("wayland-raw-appid"))?;
	xdg_toplevel.borrow_mut().set_title(String::from("wayland-raw-title"))?;

	let buf = Buffer::new_initalized(shm_pool.clone(), (0, W, H), pf, ctx.clone())?;
	surface.borrow_mut().attach_buffer_obj(buf.clone())?;
	surface.borrow_mut().commit()?;
	let mut frame: usize = 0;
	let mut cb: Option<RcCell<Callback>> = None;

	let (img_w, img_h, machine) = parse_pix("pix.ppm")?;
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

				let (r, g, b) = hsv_to_rgb((frame % 360) as f64, 1.0, 1.0);

				unsafe {
					frame = frame.wrapping_add(1);

					let slice = &mut *shm_pool.borrow_mut().slice.unwrap();
					let sur = surface.borrow();
					let buf = sur.attached_buf.clone().ok_or("no buffer")?;
					let buf = buf.borrow();

					let start_x = buf.width as isize / 2 - img_w as isize / 2;
					let start_y = buf.height as isize / 2 - img_h as isize / 2;

					for y in 0..buf.height as usize {
						for x in 0..buf.width as usize {
							let surface_ix = (buf.width as usize * y + x) * 4;

							let rel_x = x as isize - start_x;
							let rel_y = y as isize - start_y;

							if rel_x >= 0 && rel_x < img_w as isize && rel_y >= 0 && rel_y < img_h as isize {
								let img_ix = (rel_y as usize * img_w + rel_x as usize) * 3;
								slice[surface_ix + 2] = machine[img_ix];
								slice[surface_ix + 1] = machine[img_ix + 1];
								slice[surface_ix] = machine[img_ix + 2];
							} else {
								slice[surface_ix] = b;
								slice[surface_ix + 1] = g;
								slice[surface_ix + 2] = r;
							}
						}
					}
				}
				surface.borrow_mut().attach_buffer()?;
				surface.borrow_mut().repaint()?;
				surface.borrow_mut().commit()?;
			}
		}
	}
}

fn parse_pix(path: &str) -> Result<(usize, usize, Vec<u8>), Box<dyn Error>> {
	let file = File::open(path)?;
	let mut rd = BufReader::new(file);

	let mut buf = String::new();
	rd.read_line(&mut buf)?;
	if buf.trim_end() != "P6" {
		// println!("{:?}", unsafe { buf.as_mut_vec() });
		return Err("file format not ppm".into());
	}

	buf.clear();
	rd.read_line(&mut buf)?;
	let mut nonws = buf.split_whitespace();
	let w = nonws.next().ok_or("parsing w failed")?.parse()?;
	let h = nonws.next().ok_or("parsing h failed")?.parse()?;

	rd.skip_until(b'\n')?;

	let mut raster: Vec<u8> = vec![];
	rd.read_to_end(&mut raster)?;

	Ok((w, h, raster))
}

// stolen from hsv library
#[allow(dead_code)]
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
