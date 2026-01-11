use std::{cell::RefCell, error::Error, rc::Rc};

use crate::wayland::{
	CtxType, RcCell, WaylandObject, WaylandObjectKind,
	registry::Registry,
	surface::Surface,
	wire::{Id, WireArgument, WireRequest},
};

pub struct Compositor {
	pub id: Id,
	ctx: CtxType,
}

impl Compositor {
	pub fn new(id: Id, ctx: CtxType) -> Self {
		Self {
			id,
			ctx,
		}
	}

	pub fn new_bound(
		registry: &mut Registry,
		ctx: CtxType,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let compositor = Rc::new(RefCell::new(Self::new(0, ctx.clone())));
		let id = ctx
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::Compositor, compositor.clone());
		compositor.borrow_mut().id = id;
		registry.bind(id, WaylandObjectKind::Compositor, 5)?;
		Ok(compositor)
	}

	fn wl_create_surface(&self, id: Id) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![WireArgument::UnInt(id)],
		})
	}

	pub fn make_surface(&self) -> Result<RcCell<Surface>, Box<dyn Error>> {
		let surface = Rc::new(RefCell::new(Surface::new(0, self.ctx.clone())));
		let id = self
			.ctx
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::Surface, surface.clone());
		surface.borrow_mut().id = id;
		self.wl_create_surface(id)?;
		Ok(surface)
	}
}

impl WaylandObject for Compositor {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		todo!()
	}
}
