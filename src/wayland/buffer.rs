use std::{cell::RefCell, error::Error, os::fd::OwnedFd, rc::Rc};

use crate::{
	NONE, WHITE,
	wayland::{
		DebugLevel, EventAction, ExpectRc, IdentManager, OpCode, RcCell, WaylandError,
		WaylandObject, WaylandObjectKind, WeakCell,
		surface::Surface,
		wire::{Id, QueueEntry, WireRequest},
	},
	wlog,
};

pub(crate) trait BufferBackend {
	fn allocate_buffer(
		&self,
		buf: &RcCell<Buffer<Self>>,
	) -> Result<Vec<QueueEntry>, Box<dyn Error>>
	where
		Self: Sized;
	fn get_slice(&self) -> Result<*mut [u8], Box<dyn Error>>;
}

pub(crate) struct Buffer<B: BufferBackend> {
	pub(crate) id: Id,
	pub(crate) offset: i32,
	pub(crate) width: i32,
	pub(crate) height: i32,
	pub(crate) in_use: bool,
	pub(crate) backend: WeakCell<B>,
	pub(crate) master: WeakCell<Surface<B>>,
}

impl<B: BufferBackend + 'static> Buffer<B> {
	pub(crate) fn new(
		backend: &RcCell<B>,
		surface: &RcCell<Surface<B>>,
		(offset, width, height): (i32, i32, i32),
		id: Id,
	) -> RcCell<Buffer<B>> {
		Rc::new(RefCell::new(Buffer {
			id,
			offset,
			width,
			height,
			in_use: false,
			backend: Rc::downgrade(backend),
			master: Rc::downgrade(surface),
		}))
	}

	pub(crate) fn new_registered(
		wlim: &mut IdentManager,
		backend: &RcCell<B>,
		surface: &RcCell<Surface<B>>,
		(offset, width, height): (i32, i32, i32),
	) -> RcCell<Buffer<B>> {
		let buf = Self::new(backend, surface, (offset, width, height), 0);
		let id = wlim.new_id_registered(buf.borrow().kind(), buf.clone());
		buf.borrow_mut().id = id;
		buf
	}

	pub(crate) fn new_made(
		backend: &RcCell<B>,
		surface: &RcCell<Surface<B>>,
		(offset, width, height): (i32, i32, i32),
		id: Id,
	) -> Result<(RcCell<Buffer<B>>, Vec<EventAction>), Box<dyn Error>> {
		let buf = Self::new(backend, surface, (offset, width, height), id);
		let acts = backend.borrow().allocate_buffer(&buf)?;
		Ok((buf, acts))
	}

	pub(crate) fn make(
		&self,
		this: &RcCell<Buffer<B>>,
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		self.backend.upgrade().to_wl_err()?.borrow().allocate_buffer(this)
	}

	pub fn wl_destroy(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		}
	}

	pub fn destroy(&self) -> Vec<EventAction> {
		vec![EventAction::Request(self.wl_destroy())]
	}

	pub(crate) fn get_resize_actions(
		&mut self,
		new_buf_id: Id,
		(w, h): (i32, i32),
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		// todo is this needed?
		self.width = w;
		self.height = h;
		wlog!(
			DebugLevel::Important,
			self.kind_as_str(),
			format!("RESIZE w: {} h: {}", self.width, self.height),
			WHITE,
			NONE
		);

		pending.push(EventAction::Request(self.wl_destroy()));

		let backend = self.backend.upgrade().to_wl_err()?;
		let surface = self.master.upgrade().to_wl_err()?;

		let (_buf, acts) =
			Self::new_made(&backend, &surface, (self.offset, self.width, self.height), new_buf_id)?;

		pending.extend(acts);

		Ok(pending)
	}

	pub(crate) fn get_slice(&self) -> Result<*mut [u8], Box<dyn Error>> {
		self.backend.upgrade().to_wl_err()?.borrow().get_slice()
	}
}

impl<B: BufferBackend> WaylandObject for Buffer<B> {
	fn handle(
		&mut self,
		opcode: super::OpCode,
		_payload: &[u8],
		_fds: &[OwnedFd],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			// release
			0 => {
				self.in_use = false;
				pending.push(EventAction::DebugMessage(
					DebugLevel::Trivial,
					format!("{} not in use anymore", self.kind_as_str()),
				))
			}
			inv => {
				return Err(WaylandError::InvalidOpCode(inv as OpCode, self.kind_as_str()).boxed());
			}
		};
		Ok(pending)
	}

	#[inline]
	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::Buffer
	}

	#[inline]
	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}
