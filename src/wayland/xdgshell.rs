use std::{cell::RefCell, error::Error, rc::Rc};

use crate::wayland::{
	CtxType, RcCell, WaylandObject, WaylandObjectKind, registry::Registry, wire::{Id, WireArgument, WireRequest}
};

pub struct XdgWmBase {
	pub id: Id,
	ctx: CtxType,
}

impl XdgWmBase {
	pub fn new_bound(
		registry: &mut Registry,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let obj = Rc::new(RefCell::new(Self { id: 0, ctx: registry.ctx.clone() }));
		let id = registry.ctx.borrow_mut().wlim.new_id_registered(
			WaylandObjectKind::XdgWmBase,
			obj.clone()
		);
		obj.borrow_mut().id = id;
		registry.bind(id, WaylandObjectKind::XdgWmBase, 1)?;
		Ok(obj)
	}

	pub(crate) fn wl_destroy(&self) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		})
	}

	pub(crate) fn wl_pong(&self, serial: u32) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 3,
			args: vec![WireArgument::UnInt(serial)],
		})
	}

	pub fn destroy(&self) -> Result<(), Box<dyn Error>> {
		self.wl_destroy()?;
		self.ctx.borrow_mut().wlim.free_id(self.id)?;
		Ok(())
	}

	pub(crate) fn wl_get_xdg_surface(
		&self,
		wl_surface_id: Id,
		xdg_surface_id: Id,
	) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 2,
			args: vec![WireArgument::NewId(xdg_surface_id), WireArgument::Obj(wl_surface_id)],
		})
	}

	pub fn make_xdg_surface(&self, wl_surface_id: Id) -> Result<RcCell<XdgSurface>, Box<dyn Error>> {
		let xdgs = Rc::new(RefCell::new(XdgSurface { id: 0, ctx: self.ctx.clone() }));
		let id = self.ctx.borrow_mut().wlim.new_id_registered(
			WaylandObjectKind::XdgSurface,
			xdgs.clone(),
		);
		self.wl_get_xdg_surface(wl_surface_id, id)?;
		xdgs.borrow_mut().id = id;
		Ok(xdgs)
	}
}

pub struct XdgSurface {
	pub id: Id,
	ctx: CtxType,
}

impl XdgSurface {
	pub(crate) fn wl_get_toplevel(&self, xdg_toplevel_id: Id) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::NewId(xdg_toplevel_id)],
		})
	}

	pub fn make_xdg_toplevel(&self) -> Result<RcCell<XdgTopLevel>, Box<dyn Error>> {
		let xdgtl = Rc::new(RefCell::new(XdgTopLevel { id: 0, ctx: self.ctx.clone() }));
		let id = self.ctx.borrow_mut().wlim.new_id_registered(
			WaylandObjectKind::XdgTopLevel,
			xdgtl.clone(),
		);
		self.wl_get_toplevel(id)?;
		xdgtl.borrow_mut().id = id;
		Ok(xdgtl)
	}
}

pub struct XdgTopLevel {
	pub id: Id,
	ctx: CtxType,
}

impl WaylandObject for XdgWmBase {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		todo!()
	}
}

impl WaylandObject for XdgSurface {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		todo!()
	}
}

impl WaylandObject for XdgTopLevel {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		todo!()
	}
}
