use std::{cell::RefCell, error::Error, os::fd::OwnedFd, rc::Rc};

use crate::wayland::{
	EventAction, ExpectRc, God, RcCell, WaylandObject, WaylandObjectKind, WeRcGod,
	registry::Registry,
	shm::PixelFormat,
	surface::Surface,
	wire::{Id, WireArgument, WireRequest},
};

pub(crate) struct Compositor {
	pub(crate) id: Id,
	pub(crate) god: WeRcGod,
}

impl Compositor {
	pub(crate) fn new(id: Id, god: WeRcGod) -> Self {
		Self {
			id,
			god,
		}
	}

	pub fn new_bound(
		registry: RcCell<Registry>,
		god: RcCell<God>,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let compositor = Rc::new(RefCell::new(Self::new(0, Rc::downgrade(&god))));
		let id = god
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::Compositor, compositor.clone());
		compositor.borrow_mut().id = id;
		registry.borrow_mut().bind(id, WaylandObjectKind::Compositor, 5)?;
		Ok(compositor)
	}

	fn wl_create_surface(&self, id: Id) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![WireArgument::UnInt(id)],
		}
	}

	pub fn make_surface(&self) -> Result<RcCell<Surface>, Box<dyn Error>> {
		// TODO allow choice by user
		let surface =
			Rc::new(RefCell::new(Surface::new(0, PixelFormat::Argb888, self.god.clone())));
		let god = self.god.upgrade().to_wl_err()?;
		let mut god = god.borrow_mut();
		let id = god.wlim.new_id_registered(WaylandObjectKind::Surface, surface.clone());
		surface.borrow_mut().id = id;
		drop(god);
		self.queue_request(self.wl_create_surface(id))?;
		Ok(surface)
	}
}

impl WaylandObject for Compositor {
	fn id(&self) -> Id {
		self.id
	}

	fn god(&self) -> WeRcGod {
		self.god.clone()
	}

	fn handle(
		&mut self,
		_opcode: super::OpCode,
		_payload: &[u8],
		_fds: &[OwnedFd],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		todo!()
	}

	#[inline]
	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::Compositor
	}

	#[inline]
	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}
