use std::{cell::RefCell, error::Error, rc::Rc};

use crate::{
	CYAN, DebugLevel, RED,
	abstraction::wizard::TopLevelWindowWizard,
	init_logger, wait_for_sync,
	wayland::{
		EventAction, ExpectRc, God, RcCell, WaylandObject, WaylandObjectKind, WeRcGod, WeakCell,
		buffer::BufferBackend,
		callback::Callback,
		compositor::Compositor,
		display::Display,
		registry::Registry,
		shm::{PixelFormat, SharedMemory},
		surface::Surface,
		xdg_shell::{xdg_surface::XdgSurface, xdg_toplevel::XdgTopLevel, xdg_wm_base::XdgWmBase},
	},
	wlog,
};

pub use crate::wayland::buffer::BufferBackendKind;

#[allow(dead_code)]
pub struct App {
	pub(crate) pres_id_ctr: usize,
	pub(crate) presenters: Vec<(usize, RcCell<Presenter>)>,
	pub(crate) surfaces: Vec<RcCell<Surface>>,
	pub(crate) shm: RcCell<SharedMemory>,
	pub(crate) compositor: RcCell<Compositor>,
	pub(crate) registry: RcCell<Registry>,
	pub(crate) display: RcCell<Display>,
	pub finished: bool,
	pub(crate) god: RcCell<God>,
}

impl App {
	// todo appwizard with pixel format spec
	pub fn new() -> Result<Self, Box<dyn Error>> {
		init_logger();

		let god = God::new_default()?;
		let display = Display::new(god.clone())?;
		let registry = display.borrow_mut().make_registry()?;
		// fill the registry
		wait_for_sync!(display, god);
		let compositor = Compositor::new_bound(registry.clone(), god.clone())?;
		let shm = SharedMemory::new_bound_initialized(registry.clone(), god.clone())?;
		wait_for_sync!(display, god);

		Ok(Self {
			god,
			display,
			registry,
			compositor,
			surfaces: vec![],
			shm,
			presenters: vec![],
			finished: false,
			pres_id_ctr: 0,
		})
	}

	pub fn push_presenter(&mut self, presenter: Presenter) -> Result<usize, Box<dyn Error>> {
		match &presenter.medium {
			Medium::Window(tlw) => {
				tlw.surface.borrow_mut().commit()?;
			}
		};
		let presenter = Rc::new(RefCell::new(presenter));
		self.pres_id_ctr += 1;
		self.presenters.push((self.pres_id_ctr, presenter.clone()));
		Ok(self.pres_id_ctr)
	}

	pub(crate) fn make_surface(&mut self) -> Result<RcCell<Surface>, Box<dyn Error>> {
		self.compositor.borrow_mut().make_surface()
	}

	// this state thing kinda stupid
	pub fn work<F, S>(&mut self, state: &mut S, mut render_fun: F) -> Result<bool, Box<dyn Error>>
	where
		F: FnMut(&mut S, Snapshot),
	{
		for (id, presenter) in &self.presenters {
			let mut presenter = presenter.borrow_mut();
			// assume top level window for now
			let Medium::Window(ref mut window) = presenter.medium;

			let cb = &mut window.frame_cb;
			let frame = &mut window.frame;
			self.god.borrow_mut().handle_events()?;

			// check if user wants to close window - the cb might not be a good idea
			if window.xdg_toplevel.borrow().close_requested && (window.close_cb)() {
				presenter.finished = true;
				continue;
			};
			if window.xdg_surface.borrow().is_configured {
				let ready = match &cb.clone() {
					Some(cb) => cb.borrow().done,
					None => true,
				};

				let mut surf = window.surface.borrow_mut();
				if surf.attached_buf.is_none() {
					let surf_w = surf.w;
					let surf_h = surf.h;
					drop(surf);
					let buf = window.backend.make_buffer(surf_w, surf_h, &window.surface)?;
					let id = self
						.god
						.borrow_mut()
						.wlim
						.new_id_registered(WaylandObjectKind::Buffer, buf.clone());
					let acts = buf.borrow_mut().get_resize_actions(id, (surf_w, surf_h))?;
					match &window.backend {
						BufferBackend::SharedMemory(weak) => {
							let shmp = weak.upgrade().to_wl_err()?;
							let shmp = shmp.borrow();
							for (act, _, _) in acts {
								if let EventAction::Request(req) = act {
									self.god.borrow_mut().wlmm.queue_request(req, shmp.kind());
								}
							}
						}
						BufferBackend::Dma(weak) => {
							let dmabuf = weak.upgrade().to_wl_err()?;
							let dmabuf = dmabuf.borrow();
							for (act, _, _) in acts {
								if let EventAction::Request(req) = act {
									self.god.borrow_mut().wlmm.queue_request(req, dmabuf.kind());
								}
							}
						}
					}
					let mut surf = window.surface.borrow_mut();
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
						let slice = &mut *surf.get_buffer_slice()?;
						let buf = surf.attached_buf.clone().ok_or("no buffer")?;
						let buf = buf.borrow();

						let ss = Snapshot {
							buf: slice,
							w: buf.width,
							h: buf.height,
							pf: surf.pf,
							frame: *frame,
							presenter_id: *id,
						};

						render_fun(state, ss);
					}
					surf.attach_buffer()?;
					surf.repaint()?;
					surf.commit()?;
				}
			}
		}
		self.presenters.retain(|pres| !pres.1.borrow().finished);
		if self.presenters.iter().all(|(_, p)| p.borrow().finished) {
			self.finished = true;
		};
		Ok(self.finished)
	}
}

pub struct Presenter {
	pub(crate) medium: Medium,
	pub finished: bool,
}

pub enum Medium {
	Window(TopLevelWindow),
}

#[allow(dead_code)]
pub struct TopLevelWindow {
	pub(crate) xdg_toplevel: RcCell<XdgTopLevel>,
	pub(crate) xdg_surface: RcCell<XdgSurface>,
	pub(crate) xdg_wm_base: RcCell<XdgWmBase>,
	pub(crate) backend: BufferBackend,
	pub(crate) shm: WeakCell<SharedMemory>,
	pub(crate) surface: RcCell<Surface>,
	pub(crate) close_cb: Box<dyn FnMut() -> bool>,
	pub(crate) frame: usize,
	pub(crate) frame_cb: Option<RcCell<Callback>>,
	pub(crate) god: WeRcGod,
}

impl TopLevelWindow {
	pub fn spawner<'a>(parent: &'a mut App) -> TopLevelWindowWizard<'a> {
		TopLevelWindowWizard::new(parent)
	}
}

pub struct Snapshot<'a> {
	pub buf: &'a mut [u8],
	pub w: i32,
	pub h: i32,
	pub pf: PixelFormat,
	pub frame: usize,
	pub presenter_id: usize,
}

impl Drop for App {
	fn drop(&mut self) {
		// let mut god = self.god.borrow_mut();
		// let len = god.wlim.idmap.len();
		// wlog!(
		// 	DebugLevel::Important,
		// 	"app",
		// 	format!("dropping self and clearing wlim's idmap's {len} objects"),
		// 	RED,
		// 	CYAN
		// );
		// god.wlim.idmap.clear();
	}
}

impl Drop for TopLevelWindow {
	fn drop(&mut self) {
		wlog!(
			DebugLevel::Important,
			"toplevelwindow",
			"dropping self and removing relevant objects from idmap",
			RED,
			CYAN
		);
		let god = self.god.upgrade().unwrap();
		let mut god = god.borrow_mut();
		god.wlim.idmap.remove(&self.xdg_toplevel.borrow().id);
		god.wlim.idmap.remove(&self.xdg_surface.borrow().id);
		god.wlim.idmap.remove(&self.xdg_wm_base.borrow().id);
		match self.surface.borrow().attached_buf.clone().to_wl_err() {
			Ok(b) => {
				god.wlim.idmap.remove(&b.borrow().id);
			}
			Err(er) => wlog!(
				DebugLevel::Error,
				"toplevelwindow",
				format!("failed to remove {}, error: {}", WaylandObjectKind::Buffer.as_str(), er),
				RED,
				RED
			),
		};
		god.wlim.idmap.remove(&self.surface.borrow().id);
	}
}
