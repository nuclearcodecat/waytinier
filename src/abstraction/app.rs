use std::{cell::RefCell, error::Error, rc::Rc};

use crate::{
	abstraction::wizard::TopLevelWindowWizard,
	init_logger, wait_for_sync,
	wayland::{
		EventAction, God, RcCell, WaylandObjectKind, WeRcGod,
		buffer::{Buffer, BufferBackend},
		callback::Callback,
		compositor::Compositor,
		display::Display,
		registry::Registry,
		shm::{PixelFormat, SharedMemory},
		surface::Surface,
		wire::QueueEntry,
		xdg_shell::{xdg_surface::XdgSurface, xdg_toplevel::XdgTopLevel, xdg_wm_base::XdgWmBase},
	},
};

#[allow(dead_code)]
pub struct App<B: BufferBackend> {
	pub(crate) pres_id_ctr: usize,
	pub(crate) presenters: Vec<(usize, RcCell<Presenter<B>>)>,
	pub(crate) surfaces: Vec<RcCell<Surface<B>>>,
	pub(crate) shm: RcCell<SharedMemory>,
	pub(crate) compositor: RcCell<Compositor>,
	pub(crate) registry: RcCell<Registry>,
	pub(crate) display: RcCell<Display>,
	pub finished: bool,
	pub(crate) god: RcCell<God>,
}

impl<B: BufferBackend + 'static> App<B> {
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

	pub fn push_presenter(&mut self, presenter: Presenter<B>) -> Result<usize, Box<dyn Error>> {
		match &presenter.medium {
			Medium::Window(tlw) => {
				tlw.surface.borrow_mut().commit();
			}
		};
		let presenter = Rc::new(RefCell::new(presenter));
		self.pres_id_ctr += 1;
		self.presenters.push((self.pres_id_ctr, presenter.clone()));
		Ok(self.pres_id_ctr)
	}

	// this state thing kinda stupid
	pub fn work<F, S>(&mut self, state: &mut S, mut render_fun: F) -> Result<bool, Box<dyn Error>>
	where
		F: FnMut(&mut S, Snapshot),
	{
		// todo don't clone this
		for (id, presenter) in self.presenters.clone() {
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

				let (surf_w, surf_h, att_buf) = {
					let surf = window.surface.borrow_mut();
					(surf.w, surf.h, surf.attached_buf.clone())
				};
				if att_buf.is_none() {
					let buf = {
						let mut god = self.god.borrow_mut();
						Buffer::new_registered(
							&mut god.wlim,
							&window.backend,
							&window.surface,
							(0, surf_w, surf_h),
						)
					};
					self.queue(window.backend.borrow_mut().allocate_buffer(&buf)?);
					{
						let mut surf = window.surface.borrow_mut();
						self.queue(surf.attach_buffer_obj_and_att(buf)?);
						self.queue(surf.commit());
					}
					self.god.borrow_mut().handle_events()?;
					continue;
				}

				if ready {
					let mut surf = window.surface.borrow_mut();
					let (new_cb, qe) = {
						let mut god = self.god.borrow_mut();
						surf.frame(&mut god.wlim)
					};
					self.queue(qe);
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
							presenter_id: id,
						};

						render_fun(state, ss);
					}
					self.queue(surf.attach_buffer()?);
					self.queue(surf.repaint()?);
					self.queue(surf.commit());
				}
			}
		}
		self.presenters.retain(|pres| !pres.1.borrow().finished);
		if self.presenters.iter().all(|(_, p)| p.borrow().finished) {
			self.finished = true;
		};
		Ok(self.finished)
	}

	fn queue(&mut self, reqs: Vec<QueueEntry>) {
		self.god.borrow_mut().wlmm.q.extend(reqs);
	}
}

pub struct Presenter<B: BufferBackend> {
	pub(crate) medium: Medium<B>,
	pub finished: bool,
}

pub enum Medium<B: BufferBackend> {
	Window(TopLevelWindow<B>),
}

#[allow(dead_code)]
pub struct TopLevelWindow<B: BufferBackend> {
	pub(crate) xdg_toplevel: RcCell<XdgTopLevel>,
	pub(crate) xdg_surface: RcCell<XdgSurface>,
	pub(crate) xdg_wm_base: RcCell<XdgWmBase>,
	pub(crate) backend: RcCell<B>,
	pub(crate) surface: RcCell<Surface<B>>,
	pub(crate) close_cb: Box<dyn FnMut() -> bool>,
	pub(crate) frame: usize,
	pub(crate) frame_cb: Option<RcCell<Callback>>,
	pub(crate) god: WeRcGod,
}

impl<B: BufferBackend> TopLevelWindow<B> {
	pub fn spawner<'a>(parent: &'a mut App<B>) -> TopLevelWindowWizard<'a> {
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
