use std::error::Error;

use crate::wayland::{
	CtxType, WaylandObject,
	wire::{Id, WireArgument, WireRequest},
};

pub struct Surface {
	pub id: Id,
	pub(crate) ctx: CtxType,
	pub(crate) attached_buf: Option<u32>,
}

impl Surface {
	pub(crate) fn new(id: Id, ctx: CtxType) -> Self {
		Self {
			id,
			ctx,
			attached_buf: None,
		}
	}

	pub(crate) fn wl_destroy(&self) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		})
	}

	pub fn destroy(&self) -> Result<(), Box<dyn Error>> {
		self.wl_destroy()?;
		self.ctx.borrow_mut().wlim.free_id(self.id)?;
		Ok(())
	}

	pub(crate) fn wl_attach(&self, buf_id: Id) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::Obj(buf_id), WireArgument::UnInt(0), WireArgument::UnInt(0)],
		})
	}

	pub fn attach_buffer(&mut self, to_att: u32) -> Result<(), Box<dyn Error>> {
		self.attached_buf = Some(to_att);
		self.wl_attach(to_att)
	}

	pub(crate) fn wl_commit(&self) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 6,
			args: vec![],
		})
	}

	pub fn commit(&self) -> Result<(), Box<dyn Error>> {
		self.wl_commit()
	}
}

impl WaylandObject for Surface {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		todo!()
	}
}
