// todo
// - stuff everything into lib

#![allow(dead_code)]
use std::{
	cell::RefCell,
	error::Error,
	fs::File,
	io::{BufRead, BufReader, Read},
	rc::Rc,
};

use wayland_raw::{
	init_logger,
	wayland::{
		ExpectRc, God, RcCell, WeakCell,
		buffer::Buffer,
		callback::Callback,
		compositor::Compositor,
		display::Display,
		registry::Registry,
		shm::{PixelFormat, SharedMemory, SharedMemoryPool},
		surface::Surface,
		xdgshell::{XdgSurface, XdgTopLevel, XdgWmBase},
	},
};

struct App {
	god: RcCell<God>,
	display: RcCell<Display>,
	registry: RcCell<Registry>,
	compositor: RcCell<Compositor>,
	surfaces: Vec<RcCell<Surface>>,
	shm: RcCell<SharedMemory>,
	media: Vec<RcCell<Medium>>,
}

impl App {
	fn new() -> Result<Self, Box<dyn Error>> {
		init_logger();

		let god = God::new_default()?;
		let display = Display::new(god.clone())?;
		let registry = display.borrow_mut().make_registry()?;
		// fill the registry
		god.borrow_mut().handle_events()?;
		let compositor = Compositor::new_bound(registry.clone(), god.clone())?;
		let shm = SharedMemory::new_bound_initialized(registry.clone(), god.clone())?;

		Ok(Self {
			god,
			display,
			registry,
			compositor,
			surfaces: vec![],
			shm,
			media: vec![],
		})
	}

	fn push_medium(&mut self, medium: Medium) -> Result<RcCell<Medium>, Box<dyn Error>> {
		match &medium {
			Medium::Window(tlw) => {
				tlw.surface.upgrade().to_wl_err()?.borrow_mut().commit()?;
			}
		};
		let medium = Rc::new(RefCell::new(medium));
		self.media.push(medium.clone());
		Ok(medium)
	}

	fn make_surface(&mut self) -> Result<RcCell<Surface>, Box<dyn Error>> {
		self.compositor.borrow_mut().make_surface()
	}

	fn work<F, S>(&mut self, state: &mut S, mut render_fun: F) -> Result<(), Box<dyn Error>>
	where
		F: FnMut(&mut S, Snapshot),
	{
		let mut window = self.media[0].borrow_mut();
		let Medium::Window(ref mut window) = *window;
		let mut frame: usize = 0;

		let mut cb: Option<RcCell<Callback>> = None;
		loop {
			self.god.borrow_mut().handle_events()?;

			// check if user wants to close window - the cb might not be a good idea
			if window.xdg_toplevel.borrow().close_requested && (window.close_cb)() {
				break Ok(());
			};
			if window.xdg_surface.borrow().is_configured {
				let ready = match &cb.clone() {
					Some(cb) => cb.borrow().done,
					None => true,
				};

				let surf = window.surface.upgrade().to_wl_err()?;
				let mut surf = surf.borrow_mut();
				if surf.attached_buf.is_none() {
					let buf = Buffer::new_initalized(
						window.shm_pool.clone(),
						(0, surf.w, surf.h),
						PixelFormat::Xrgb888,
						self.god.clone(),
					)?;
					surf.attach_buffer_obj(buf)?;
					surf.commit()?;
					drop(surf);
					self.god.borrow_mut().handle_events()?;
					continue;
				}

				if ready {
					let new_cb = surf.frame()?;
					cb = Some(new_cb);
					frame = frame.wrapping_add(1);

					unsafe {
						let slice = &mut *window.shm_pool.borrow_mut().slice.unwrap();
						let buf = surf.attached_buf.clone().ok_or("no buffer")?;
						let buf = buf.borrow();

						let ss = Snapshot {
							buf: slice,
							w: buf.width,
							h: buf.height,
							pf: buf.format,
							frame,
						};

						render_fun(state, ss);
					}
					surf.attach_buffer()?;
					surf.repaint()?;
					surf.commit()?;
				}
			}
		}
	}
}

enum Medium {
	Window(TopLevelWindow),
}

struct TopLevelWindow {
	xdg_toplevel: RcCell<XdgTopLevel>,
	xdg_surface: RcCell<XdgSurface>,
	xdg_wm_base: RcCell<XdgWmBase>,
	shm_pool: RcCell<SharedMemoryPool>,
	shm: WeakCell<SharedMemory>,
	surface: WeakCell<Surface>,
	close_cb: Box<dyn Fn() -> bool>,
}

impl TopLevelWindow {
	fn spawner<'a>(parent: &'a mut App) -> TopLevelWindowSpawner<'a> {
		TopLevelWindowSpawner::new(None, parent)
	}
}

struct TopLevelWindowSpawner<'a> {
	app_id: Option<String>,
	title: Option<String>,
	width: Option<i32>,
	height: Option<i32>,
	pf: Option<PixelFormat>,
	sur: Option<RcCell<Surface>>,
	parent: &'a mut App,
	close_cb: Option<Box<dyn Fn() -> bool>>,
}

impl<'a> TopLevelWindowSpawner<'a> {
	fn with_app_id(&mut self, app_id: String) {
		self.app_id = Some(app_id);
	}

	fn with_title(&mut self, title: String) {
		self.title = Some(title);
	}

	fn with_width(&mut self, width: i32) {
		self.width = Some(width);
	}

	fn with_height(&mut self, height: i32) {
		self.height = Some(height);
	}

	fn with_pixel_format(&mut self, pf: PixelFormat) {
		self.pf = Some(pf);
	}

	fn with_premade_surface(&mut self, wl_surface: RcCell<Surface>) {
		self.sur = Some(wl_surface);
	}

	fn with_close_callback(&mut self, cb: Box<dyn Fn() -> bool>) {
		self.close_cb = Some(cb);
	}

	fn new(wl_surface: Option<RcCell<Surface>>, parent: &'a mut App) -> Self {
		Self {
			sur: wl_surface,
			parent,
			app_id: None,
			title: None,
			width: None,
			height: None,
			pf: None,
			close_cb: None,
		}
	}

	fn spawn(self) -> Result<Medium, Box<dyn Error>> {
		let w = self.width.unwrap_or(800);
		let h = self.width.unwrap_or(600);
		let pf = self.pf.unwrap_or(PixelFormat::Xrgb888);
		let surface = if let Some(sur) = &self.sur {
			sur
		} else {
			&self.parent.compositor.borrow_mut().make_surface()?
		};
		let close_cb = if let Some(fun) = self.close_cb {
			fun
		} else {
			Box::new(|| true)
		};
		let shm_pool = self.parent.shm.borrow_mut().make_pool(w * h * pf.width())?;
		let xdg_wm_base = XdgWmBase::new_bound(self.parent.registry.clone())?;
		let xdg_surface = xdg_wm_base.borrow_mut().make_xdg_surface(surface.clone())?;
		let xdg_toplevel =
			XdgTopLevel::new_from_xdg_surface(xdg_surface.clone(), self.parent.god.clone())?;
		{
			let mut xdgtl = xdg_toplevel.borrow_mut();
			if let Some(x) = &self.app_id {
				xdgtl.set_app_id(x)?;
			};
			if let Some(x) = &self.title {
				xdgtl.set_title(x)?;
			};
		}
		Ok(Medium::Window(TopLevelWindow {
			xdg_toplevel,
			xdg_surface,
			xdg_wm_base,
			shm_pool,
			shm: Rc::downgrade(&self.parent.shm),
			surface: Rc::downgrade(surface),
			close_cb,
		}))
	}
}

struct Snapshot<'a> {
	buf: &'a mut [u8],
	w: i32,
	h: i32,
	pf: PixelFormat,
	frame: usize,
}

// user-made struct, hide all of the above in lib
struct AppState {
	img_w: usize,
	img_h: usize,
	machine: Vec<u8>,
}

fn main() -> Result<(), Box<dyn Error>> {
	let mut app = App::new()?;
	let window = TopLevelWindow::spawner(&mut app).spawn()?;
	app.push_medium(window)?;

	let (img_w, img_h, machine) = parse_pix("pix.ppm")?;
	let mut state = AppState {
		img_w,
		img_h,
		machine,
	};

	app.work(&mut state, |state, ss| {
		let (r, g, b) = hsv_to_rgb((ss.frame % 360) as f64, 1.0, 1.0);
		let start_x = ss.w as isize / 2 - state.img_w as isize / 2;
		let start_y = ss.h as isize / 2 - state.img_h as isize / 2;

		for y in 0..ss.h as usize {
			for x in 0..ss.w as usize {
				let surface_ix = (ss.w as usize * y + x) * 4;

				let rel_x = x as isize - start_x;
				let rel_y = y as isize - start_y;

				if rel_x >= 0
					&& rel_x < state.img_w as isize
					&& rel_y >= 0 && rel_y < state.img_h as isize
				{
					let img_ix = (rel_y as usize * img_w + rel_x as usize) * 3;
					ss.buf[surface_ix + 2] = state.machine[img_ix];
					ss.buf[surface_ix + 1] = state.machine[img_ix + 1];
					ss.buf[surface_ix] = state.machine[img_ix + 2];
				} else {
					ss.buf[surface_ix] = b.wrapping_sub(x as u8);
					ss.buf[surface_ix + 1] = g.wrapping_add(y as u8);
					ss.buf[surface_ix + 2] = r.wrapping_shl(x as u32);
				}
			}
		}
	})
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
