use std::{cell::RefCell, error::Error, rc::Rc};

use crate::wayland::{CtxType, RcCell, WaylandObject, wire::Id};

pub struct Callback {
	pub(crate) id: Id,
	pub(crate) ctx: CtxType,
}

impl Callback {
	pub(crate) fn new(ctx: CtxType) -> Result<RcCell<Self>, Box<dyn Error>> {
		let cb = Rc::new(RefCell::new(Self { id: 0, ctx: ctx.clone() }));
		let id = ctx.borrow_mut().wlim.new_id_registered(
			super::WaylandObjectKind::Callback,
			cb.clone(),
		);
		cb.borrow_mut().id = id;
		Ok(cb)
	}
}

impl WaylandObject for Callback {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
		todo!()
	}
}

