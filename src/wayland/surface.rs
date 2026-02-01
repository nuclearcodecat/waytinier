use std::{error::Error, os::fd::OwnedFd};

use crate::{
	make_drop_impl,
	wayland::{
		EventAction, RcCell, WaylandError, WaylandObject, WaylandObjectKind, WeRcGod,
		buffer::Buffer,
		callback::Callback,
		shm::PixelFormat,
		wire::{Id, WireArgument, WireRequest},
	},
};

pub(crate) struct Surface {
	pub(crate) id: Id,
	pub(crate) god: WeRcGod,
	pub(crate) attached_buf: Option<RcCell<Buffer>>,
	pub(crate) pf: PixelFormat,
	pub(crate) w: i32,
	pub(crate) h: i32,
}

impl Surface {
	pub(crate) fn new(id: Id, pf: PixelFormat, god: WeRcGod) -> Self {
		Self {
			id,
			god,
			attached_buf: None,
			pf,
			w: 0,
			h: 0,
		}
	}

	pub(crate) fn wl_destroy(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		}
	}

	pub fn destroy(&self) -> Result<(), Box<dyn Error>> {
		self.queue_request(self.wl_destroy())
	}

	pub(crate) fn wl_attach(&self, buf_id: Id) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::Obj(buf_id), WireArgument::UnInt(0), WireArgument::UnInt(0)],
		}
	}

	pub fn attach_buffer_obj(&mut self, to_att: RcCell<Buffer>) -> Result<(), Box<dyn Error>> {
		self.attached_buf = Some(to_att.clone());
		self.attach_buffer()
	}

	pub fn attach_buffer(&mut self) -> Result<(), Box<dyn Error>> {
		let buf = self.attached_buf.clone().ok_or(WaylandError::BufferObjectNotAttached)?;
		self.queue_request(self.wl_attach(buf.borrow().id))
	}

	pub(crate) fn wl_commit(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 6,
			args: vec![],
		}
	}

	pub fn commit(&self) -> Result<(), Box<dyn Error>> {
		self.queue_request(self.wl_commit())
	}

	pub(crate) fn wl_damage_buffer(&self, x: i32, y: i32, w: i32, h: i32) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 9,
			args: vec![
				WireArgument::Int(x),
				WireArgument::Int(y),
				WireArgument::Int(w),
				WireArgument::Int(h),
			],
		}
	}

	pub(crate) fn damage_buffer(
		&self,
		(x, y): (i32, i32),
		(w, h): (i32, i32),
	) -> Result<(), Box<dyn Error>> {
		self.queue_request(self.wl_damage_buffer(x, y, w, h))
	}

	pub(crate) fn repaint(&self) -> Result<(), Box<dyn Error>> {
		if let Some(buf) = &self.attached_buf {
			let buf = buf.borrow();
			self.queue_request(self.wl_damage_buffer(0, 0, buf.width, buf.height))
		} else {
			Err(WaylandError::BufferObjectNotAttached.boxed())
		}
	}

	pub(crate) fn wl_frame(&self, id: Id) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 3,
			args: vec![WireArgument::NewId(id)],
		}
	}

	pub(crate) fn frame(&self) -> Result<RcCell<Callback>, Box<dyn Error>> {
		let cb = Callback::new(self.god.clone())?;
		self.queue_request(self.wl_frame(cb.borrow().id))?;
		Ok(cb)
	}

	pub(crate) fn get_buffer_slice(&self) -> Result<*mut [u8], Box<dyn Error>> {
		if let Some(buf) = &self.attached_buf {
			let buf = buf.borrow_mut();
			buf.get_slice()
		} else {
			Err(WaylandError::BufferObjectNotAttached.boxed())
		}
	}
}

impl WaylandObject for Surface {
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
		WaylandObjectKind::Surface
	}

	#[inline]
	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}

make_drop_impl!(Surface, wl_destroy);
