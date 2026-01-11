use crate::wayland::{
	CtxType, OpCode, RcCell, WaylandError, WaylandObject, WaylandObjectKind, callback::Callback, registry::Registry, wire::{FromWirePayload, Id, WireArgument, WireRequest}
};
use std::{cell::RefCell, error::Error, rc::Rc};

pub struct Display {
	pub id: Id,
	ctx: CtxType,
}

impl Display {
	pub fn new(ctx: CtxType) -> RcCell<Self> {
		let display = Rc::new(RefCell::new(Self {
			id: 0,
			ctx: ctx.clone(),
		}));
		let id =
			ctx.borrow_mut().wlim.new_id_registered(WaylandObjectKind::Display, display.clone());
		display.borrow_mut().id = id;
		display
	}

	pub fn make_registry(&mut self) -> Result<RcCell<Registry>, Box<dyn Error>> {
		let reg = Rc::new(RefCell::new(Registry::new_empty(0, self.ctx.clone())));
		let id = self.ctx.borrow_mut().wlim.new_id_registered(WaylandObjectKind::Registry, reg.clone());
		reg.borrow_mut().id = id;
		self.wl_get_registry(id)?;
		Ok(reg)
	}

	pub(crate) fn wl_get_registry(&mut self, id: Id) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::NewId(id)],
		})
	}

	pub(crate) fn wl_sync(&mut self, id: Id) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![WireArgument::NewId(id)],
		})
	}
	
	pub fn sync(&mut self) -> Result<RcCell<Callback>, Box<dyn Error>> {
		let cb = Callback::new(self.ctx.clone())?;
		let id = self.ctx.borrow_mut().wlim.new_id_registered(
			WaylandObjectKind::Callback,
			cb.clone(),
		);
		self.wl_sync(id)?;
		Ok(cb)
	}
}

impl WaylandObject for Display {
	fn handle(&mut self, opcode: OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		let p = payload;
		match opcode {
			0 => {
				let obj_id = u32::from_wire(&p[8..])?;
				let code = u32::from_wire(&p[12..])?;
				let message = String::from_wire(&p[16..])?;
				// maybe add some sort of error manager
				eprintln!(
					"======== ERROR {} FIRED in wl_display\nfor object {:?}\n{:?}",
					code,
					self.ctx
						.borrow()
						.wlim
						.find_obj_kind_by_id(obj_id)
						.ok_or(WaylandError::ObjectNonExistent)?,
					message
				);
			}
			1 => {
				let deleted_id = u32::from_wire(&payload[8..])?;
				// println!(
				// 	"==================== ID {:?} GOT DELETED (unimpl)",
				// 	deleted_id
				// );
				self.ctx.borrow_mut().wlim.free_id(deleted_id)?;
			}
			_ => {
				eprintln!("invalid display event");
			}
		}
		Ok(())
	}
}
