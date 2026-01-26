use std::{cell::RefCell, error::Error, rc::Rc};

use crate::wayland::{
	Context, CtxType, EventAction, ExpectRc, RcCell, WaylandObject, WaylandObjectKind,
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
		ctx: RcCell<Context>,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let compositor = Rc::new(RefCell::new(Self::new(0, Rc::downgrade(&ctx))));
		let id = ctx
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::Compositor, compositor.clone());
		compositor.borrow_mut().id = id;
		registry.bind(id, WaylandObjectKind::Compositor, 5)?;
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
		let surface = Rc::new(RefCell::new(Surface::new(0, self.ctx.clone())));
		let ctx = self.ctx.upgrade().to_wl_err()?;
		let mut ctx = ctx.borrow_mut();
		let id = ctx.wlim.new_id_registered(WaylandObjectKind::Surface, surface.clone());
		surface.borrow_mut().id = id;
		ctx.wlmm.send_request(&mut self.wl_create_surface(id))?;
		Ok(surface)
	}
}

impl WaylandObject for Compositor {
	fn handle(
		&mut self,
		_opcode: super::OpCode,
		_payload: &[u8],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		todo!()
	}

	fn as_str(&self) -> &'static str {
		WaylandObjectKind::Compositor.as_str()
	}
}
