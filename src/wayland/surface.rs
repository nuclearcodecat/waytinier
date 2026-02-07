use std::{error::Error, os::fd::OwnedFd};

use crate::wayland::{
	EventAction, IdentManager, RcCell, WaylandError, WaylandObject, WaylandObjectKind, WeRcGod,
	buffer::{Buffer, BufferBackend},
	callback::Callback,
	shm::PixelFormat,
	wire::{Id, QueueEntry, WireArgument, WireRequest},
};

pub(crate) struct Surface<B: BufferBackend> {
	pub(crate) id: Id,
	pub(crate) god: WeRcGod,
	pub(crate) attached_buf: Option<RcCell<Buffer<B>>>,
	pub(crate) pf: PixelFormat,
	pub(crate) w: i32,
	pub(crate) h: i32,
}

impl<B: BufferBackend + 'static> Surface<B> {
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

	pub fn destroy(&self) -> Vec<EventAction> {
		vec![EventAction::Request(self.wl_destroy())]
	}

	pub(crate) fn wl_attach(&self, buf_id: Id) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::Obj(buf_id), WireArgument::UnInt(0), WireArgument::UnInt(0)],
		}
	}

	pub fn attach_buffer_obj_and_att(
		&mut self,
		to_att: RcCell<Buffer<B>>,
	) -> Result<Vec<QueueEntry>, Box<dyn Error>> {
		self.attached_buf = Some(to_att.clone());
		self.attach_buffer()
	}

	pub fn attach_buffer(&mut self) -> Result<Vec<QueueEntry>, Box<dyn Error>> {
		let buf = self.attached_buf.clone().ok_or(WaylandError::BufferObjectNotAttached)?;
		Ok(vec![QueueEntry::Request((self.wl_attach(buf.borrow().id), self.kind()))])
	}

	pub(crate) fn wl_commit(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 6,
			args: vec![],
		}
	}

	pub fn commit(&self) -> Vec<QueueEntry> {
		vec![QueueEntry::Request((self.wl_commit(), self.kind()))]
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

	pub(crate) fn damage_buffer(&self, (x, y): (i32, i32), (w, h): (i32, i32)) -> Vec<QueueEntry> {
		vec![QueueEntry::Request((self.wl_damage_buffer(x, y, w, h), self.kind()))]
	}

	pub(crate) fn repaint(&self) -> Result<Vec<QueueEntry>, Box<dyn Error>> {
		if let Some(buf) = &self.attached_buf {
			let buf = buf.borrow();
			Ok(self.damage_buffer((0, 0), (buf.width, buf.height)))
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

	pub(crate) fn frame(&self, wlim: &mut IdentManager) -> (RcCell<Callback>, Vec<QueueEntry>) {
		let cb = Callback::new_registered(wlim);
		(cb.clone(), vec![QueueEntry::Request((self.wl_frame(cb.borrow().id), self.kind()))])
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

impl<B: BufferBackend> WaylandObject for Surface<B> {
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
