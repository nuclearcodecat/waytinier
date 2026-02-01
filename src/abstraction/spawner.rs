use std::{error::Error, rc::Rc};

use crate::{
	DebugLevel, NONE, RED,
	abstraction::app::{App, Medium, Presenter, TopLevelWindow},
	wayland::{
		RcCell,
		buffer::{BufferBackend, BufferBackendKind},
		dmabuf::DmaBuf,
		surface::Surface,
		xdg_shell::{xdg_toplevel::XdgTopLevel, xdg_wm_base::XdgWmBase},
	},
	wlog,
};

#[allow(dead_code)]
pub struct TopLevelWindowSpawner<'a> {
	pub(crate) app_id: Option<String>,
	pub(crate) title: Option<String>,
	pub(crate) width: Option<i32>,
	pub(crate) height: Option<i32>,
	pub(crate) sur: Option<RcCell<Surface>>,
	pub(crate) parent: &'a mut App,
	pub(crate) close_cb: Option<Box<dyn FnMut() -> bool>>,
	pub(crate) buf_backend: Option<BufferBackendKind>,
}

impl<'a> TopLevelWindowSpawner<'a> {
	pub fn with_app_id(mut self, app_id: &str) -> Self {
		self.app_id = Some(String::from(app_id));
		self
	}

	pub fn with_title(mut self, title: &str) -> Self {
		self.title = Some(String::from(title));
		self
	}

	pub fn with_width(mut self, width: i32) -> Self {
		self.width = Some(width);
		self
	}

	pub fn with_height(mut self, height: i32) -> Self {
		self.height = Some(height);
		self
	}

	pub fn with_close_callback<F>(mut self, cb: F) -> Self
	where
		F: FnMut() -> bool + 'static,
	{
		self.close_cb = Some(Box::new(cb));
		self
	}

	pub fn with_buffer_backend(mut self, backend: BufferBackendKind) -> Self {
		self.buf_backend = Some(backend);
		self
	}

	pub(crate) fn new(parent: &'a mut App) -> Self {
		Self {
			sur: None,
			parent,
			app_id: None,
			title: None,
			width: None,
			height: None,
			close_cb: None,
			buf_backend: None,
		}
	}

	pub fn spawn(self) -> Result<Presenter, Box<dyn Error>> {
		let w = self.width.unwrap_or(800);
		let h = self.width.unwrap_or(600);
		let surface = self.parent.compositor.borrow_mut().make_surface()?;
		let backend = self.buf_backend.unwrap_or(BufferBackendKind::SharedMemory);
		let backend = match backend {
			BufferBackendKind::SharedMemory => {
				let shm_pool =
					self.parent.shm.borrow_mut().make_pool(w * h * surface.borrow().pf.width())?;
				self.parent.god.borrow_mut().handle_events()?;
				BufferBackend::from(&shm_pool)
			}
			BufferBackendKind::Dma => {
				let dmabuf_ = DmaBuf::new_bound(
					self.parent.registry.clone(),
					self.parent.god.clone(),
					&surface,
				)?;
				let mut dmabuf = dmabuf_.borrow_mut();
				let _fb = dmabuf.get_default_feedback(&dmabuf_)?;
				self.parent.god.borrow_mut().handle_events()?;
				BufferBackend::from(&dmabuf_)
			}
		};

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
		let close_cb = self.close_cb.unwrap_or(Box::new(|| {
			wlog!(DebugLevel::Important, "toplevelwindow", "close cb triggered", RED, NONE);
			true
		}));
		Ok(Presenter {
			finished: false,
			medium: Medium::Window(TopLevelWindow {
				xdg_toplevel,
				xdg_surface,
				xdg_wm_base,
				shm: Rc::downgrade(&self.parent.shm),
				backend,
				surface,
				close_cb,
				frame: 0,
				frame_cb: None,
				god: Rc::downgrade(&self.parent.god),
			}),
		})
	}
}
