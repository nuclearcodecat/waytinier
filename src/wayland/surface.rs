use std::error::Error;

use crate::wayland::{
	CtxType, EventAction, RcCell, WaylandError, WaylandObject, WaylandObjectKind, buffer::Buffer, callback::Callback, region::Region, wire::{Id, WireArgument, WireRequest}
};

pub struct Surface {
	pub id: Id,
	pub(crate) ctx: CtxType,
	pub(crate) attached_buf: Option<RcCell<Buffer>>,
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

	pub(crate) fn wl_attach(&self, buf_id: Id) -> Result<WireRequest, Box<dyn Error>> {
		Ok(WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::Obj(buf_id), WireArgument::UnInt(0), WireArgument::UnInt(0)],
		})
	}

	pub fn attach_buffer_obj(&mut self, to_att: RcCell<Buffer>) -> Result<(), Box<dyn Error>> {
		self.attached_buf = Some(to_att.clone());
		self.attach_buffer()
	}

	pub fn attach_buffer(&mut self) -> Result<(), Box<dyn Error>> {
		let buf =  self.attached_buf.clone().ok_or(WaylandError::BufferObjectNotAttached)?;
		self.ctx.borrow().wlmm.send_request(&mut self.wl_attach(buf.borrow().id)?)
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

	pub(crate) fn wl_damage_buffer(&self, region: Region) -> Result<WireRequest, Box<dyn Error>> {
		Ok(WireRequest {
			sender_id: self.id,
			opcode: 9,
			args: vec![
				WireArgument::Int(region.x),
				WireArgument::Int(region.y),
				WireArgument::Int(region.w),
				WireArgument::Int(region.h),
			],
		})
	}

	pub fn damage_buffer(&self, region: Region) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut self.wl_damage_buffer(region)?)
	}

	pub fn repaint(&self) -> Result<(), Box<dyn Error>> {
		if let Some(buf) = &self.attached_buf {
			let buf = buf.borrow();
			self.ctx.borrow().wlmm.send_request(&mut self.wl_damage_buffer(Region {
				x: 0,
				y: 0,
				w: buf.width,
				h: buf.height,
			})?)?;
		};
		Ok(())
	}

	pub(crate) fn wl_frame(&self, id: Id) -> Result<WireRequest, Box<dyn Error>> {
		Ok(WireRequest {
			sender_id: self.id,
			opcode: 3,
			args: vec![WireArgument::NewId(id)],
		})
	}

	pub fn frame(&self) -> Result<RcCell<Callback>, Box<dyn Error>> {
		let cb = Callback::new(self.ctx.clone())?;
		self.ctx.borrow().wlmm.send_request(&mut self.wl_frame(cb.borrow().id)?)?;
		Ok(cb)
	}
}

impl WaylandObject for Surface {
	fn handle(
		&mut self,
		opcode: super::OpCode,
		payload: &[u8],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		todo!()
	}

	fn as_str(&self) -> &'static str {
		WaylandObjectKind::Surface.as_str()
	}
}
