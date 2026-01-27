use std::{error::Error, rc::Rc};

use crate::{
	abstraction::app::{App, Medium, Presenter, TopLevelWindow},
	wayland::{
		RcCell,
		shm::PixelFormat,
		surface::Surface,
		xdgshell::{XdgTopLevel, XdgWmBase},
	},
};

pub struct TopLevelWindowSpawner<'a> {
	pub(crate) app_id: Option<String>,
	pub(crate) title: Option<String>,
	pub(crate) width: Option<i32>,
	pub(crate) height: Option<i32>,
	pub(crate) pf: Option<PixelFormat>,
	pub(crate) sur: Option<RcCell<Surface>>,
	pub(crate) parent: &'a mut App,
	pub(crate) close_cb: Option<Box<dyn FnMut() -> bool>>,
}

impl<'a> TopLevelWindowSpawner<'a> {
	pub fn with_app_id(&mut self, app_id: String) {
		self.app_id = Some(app_id);
	}

	pub fn with_title(&mut self, title: String) {
		self.title = Some(title);
	}

	pub fn with_width(&mut self, width: i32) {
		self.width = Some(width);
	}

	pub fn with_height(&mut self, height: i32) {
		self.height = Some(height);
	}

	pub fn with_pixel_format(&mut self, pf: PixelFormat) {
		self.pf = Some(pf);
	}

	pub fn with_premade_surface(&mut self, wl_surface: RcCell<Surface>) {
		self.sur = Some(wl_surface);
	}

	pub fn with_close_callback<F>(&mut self, cb: F)
	where
		F: FnMut() -> bool + 'static,
	{
		self.close_cb = Some(Box::new(cb));
	}

	pub(crate) fn new(wl_surface: Option<RcCell<Surface>>, parent: &'a mut App) -> Self {
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

	pub fn spawn(self) -> Result<Presenter, Box<dyn Error>> {
		let w = self.width.unwrap_or(800);
		let h = self.width.unwrap_or(600);
		let pf = self.pf.unwrap_or(PixelFormat::Xrgb888);
		let surface = if let Some(sur) = &self.sur {
			sur
		} else {
			&self.parent.compositor.borrow_mut().make_surface()?
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
		let close_cb = self.close_cb.unwrap_or(Box::new(|| true));
		Ok(Presenter {
			finished: false,
			medium: Medium::Window(TopLevelWindow {
				xdg_toplevel,
				xdg_surface,
				xdg_wm_base,
				shm_pool,
				shm: Rc::downgrade(&self.parent.shm),
				surface: Rc::downgrade(surface),
				close_cb,
				frame: 0,
				frame_cb: None,
			}),
		})
	}
}
