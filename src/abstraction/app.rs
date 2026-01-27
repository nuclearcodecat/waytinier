use std::{cell::RefCell, error::Error, rc::Rc};

use crate::{
	abstraction::spawner::TopLevelWindowSpawner,
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

pub struct App {
	pub(crate) god: RcCell<God>,
	pub(crate) display: RcCell<Display>,
	pub(crate) registry: RcCell<Registry>,
	pub(crate) compositor: RcCell<Compositor>,
	pub(crate) surfaces: Vec<RcCell<Surface>>,
	pub(crate) shm: RcCell<SharedMemory>,
	pub(crate) presenters: Vec<RcCell<Presenter>>,
	pub finished: bool,
}

impl App {
	pub fn new() -> Result<Self, Box<dyn Error>> {
		init_logger();

		let god = God::new_default()?;
		let display = Display::new(god.clone())?;
		let registry = display.borrow_mut().make_registry()?;
		// fill the registry
		god.borrow_mut().handle_events()?;
		let compositor = Compositor::new_bound(registry.clone(), god.clone())?;
		let shm = SharedMemory::new_bound_initialized(registry.clone(), god.clone())?;
		god.borrow_mut().handle_events()?;

		Ok(Self {
			god,
			display,
			registry,
			compositor,
			surfaces: vec![],
			shm,
			presenters: vec![],
			finished: false,
		})
	}

	pub fn push_presenter(
		&mut self,
		presenter: Presenter,
	) -> Result<RcCell<Presenter>, Box<dyn Error>> {
		match &presenter.medium {
			Medium::Window(tlw) => {
				tlw.surface.upgrade().to_wl_err()?.borrow_mut().commit()?;
			}
		};
		let presenter = Rc::new(RefCell::new(presenter));
		self.presenters.push(presenter.clone());
		Ok(presenter)
	}

	pub fn make_surface(&mut self) -> Result<RcCell<Surface>, Box<dyn Error>> {
		self.compositor.borrow_mut().make_surface()
	}

	pub fn work<F, S>(&mut self, state: &mut S, mut render_fun: F) -> Result<bool, Box<dyn Error>>
	where
		F: FnMut(&mut S, Snapshot),
	{
		for pres in &self.presenters {
			let mut pres = pres.borrow_mut();
			// assume top level window for now
			let Medium::Window(ref mut pres) = pres.medium;

			let cb = &mut pres.frame_cb;
			let frame = &mut pres.frame;
			self.god.borrow_mut().handle_events()?;

			// check if user wants to close window - the cb might not be a good idea
			if pres.xdg_toplevel.borrow().close_requested && (pres.close_cb)() {
				self.finished = true;
				break;
			};
			if pres.xdg_surface.borrow().is_configured {
				let ready = match &cb.clone() {
					Some(cb) => cb.borrow().done,
					None => true,
				};

				let surf = pres.surface.upgrade().to_wl_err()?;
				let mut surf = surf.borrow_mut();
				if surf.attached_buf.is_none() {
					let pf = PixelFormat::Xrgb888;
					let width = pf.width();
					let buf = Buffer::new_initalized(
						pres.shm_pool.clone(),
						(0, surf.w, surf.h),
						pf,
						self.god.clone(),
					);
					pres.shm_pool.borrow_mut().resize_if_larger(surf.w * surf.h * width)?;
					surf.attach_buffer_obj(buf)?;
					surf.commit()?;
					drop(surf);
					self.god.borrow_mut().handle_events()?;
					continue;
				}

				if ready {
					let new_cb = surf.frame()?;
					*cb = Some(new_cb);
					*frame = frame.wrapping_add(1);

					unsafe {
						let slice = &mut *pres.shm_pool.borrow_mut().slice.unwrap();
						let buf = surf.attached_buf.clone().ok_or("no buffer")?;
						let buf = buf.borrow();

						let ss = Snapshot {
							buf: slice,
							w: buf.width,
							h: buf.height,
							pf: buf.format,
							frame: *frame,
						};

						render_fun(state, ss);
					}
					surf.attach_buffer()?;
					surf.repaint()?;
					surf.commit()?;
				}
			}
		}
		Ok(self.finished)
	}
}

pub struct Presenter {
	pub finished: bool,
	pub(crate) medium: Medium,
}

pub enum Medium {
	Window(TopLevelWindow),
}

pub struct TopLevelWindow {
	pub(crate) xdg_toplevel: RcCell<XdgTopLevel>,
	pub(crate) xdg_surface: RcCell<XdgSurface>,
	pub(crate) xdg_wm_base: RcCell<XdgWmBase>,
	pub(crate) shm_pool: RcCell<SharedMemoryPool>,
	pub(crate) shm: WeakCell<SharedMemory>,
	pub(crate) surface: WeakCell<Surface>,
	pub(crate) close_cb: Box<dyn FnMut() -> bool>,
	pub(crate) frame: usize,
	pub(crate) frame_cb: Option<RcCell<Callback>>,
}

impl TopLevelWindow {
	pub fn spawner<'a>(parent: &'a mut App) -> TopLevelWindowSpawner<'a> {
		TopLevelWindowSpawner::new(None, parent)
	}
}

pub struct Snapshot<'a> {
	pub buf: &'a mut [u8],
	pub w: i32,
	pub h: i32,
	pub pf: PixelFormat,
	pub frame: usize,
}
