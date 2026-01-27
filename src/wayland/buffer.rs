use std::{cell::RefCell, error::Error, rc::Rc};

use crate::{
	NONE, WHITE,
	wayland::{
		DebugLevel, EventAction, ExpectRc, God, OpCode, RcCell, WaylandError, WaylandObject,
		WaylandObjectKind, WeRcGod, WeakCell,
		shm::{PixelFormat, SharedMemoryPool},
		wire::{Id, WireRequest},
	},
	wlog,
};

pub struct Buffer {
	pub id: Id,
	pub(crate) god: WeRcGod,
	pub offset: i32,
	pub width: i32,
	pub height: i32,
	pub format: PixelFormat,
	pub in_use: bool,
	pub shm_pool: WeakCell<SharedMemoryPool>,
}

impl Buffer {
	pub fn new_initalized(
		shmp: RcCell<SharedMemoryPool>,
		(offset, width, height): (i32, i32, i32),
		format: PixelFormat,
		god: RcCell<God>,
	) -> RcCell<Buffer> {
		let buf = Rc::new(RefCell::new(Buffer {
			id: 0,
			god: Rc::downgrade(&god),
			offset,
			width,
			height,
			format,
			in_use: false,
			shm_pool: Rc::downgrade(&shmp).clone(),
		}));
		let mut god = god.borrow_mut();
		let id = god.wlim.new_id_registered(WaylandObjectKind::Buffer, buf.clone());
		{
			let mut buf = buf.borrow_mut();
			buf.id = id;
			god.wlmm.queue_request(shmp.borrow().wl_create_buffer(
					id,
					(offset, width, height, width * format.width()),
					format,
			), buf.kind());
		}
		buf
	}

	pub(crate) fn wl_destroy(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		}
	}

	pub fn destroy(&self) -> Result<(), Box<dyn Error>> {
		let god = self.god.upgrade().to_wl_err()?;
		let mut god = god.borrow_mut();
		self.queue_request(self.wl_destroy())?;
		god.wlim.free_id(self.id)?;
		Ok(())
	}

	#[allow(clippy::type_complexity)]
	pub(crate) fn resize(
		&mut self,
		new_buf_id: Id,
		(w, h): (i32, i32),
	) -> Result<Vec<(EventAction, WaylandObjectKind, Id)>, Box<dyn Error>> {
		let mut pending = vec![];
		self.width = w;
		self.height = h;
		wlog!(
			DebugLevel::Important,
			self.kind_as_str(),
			format!("RESIZE w: {} h: {}", self.width, self.height),
			WHITE,
			NONE
		);

		pending.push((EventAction::Request(self.wl_destroy()), WaylandObjectKind::Buffer, self.id));

		let shmp = self.shm_pool.upgrade().to_wl_err()?;
		let mut shmp = shmp.borrow_mut();
		let shm_actions = shmp.resize_if_larger(w * h * self.format.width())?;
		pending.append(
			&mut shm_actions
				.into_iter()
				.map(|x| (x, WaylandObjectKind::SharedMemoryPool, shmp.id))
				.collect(),
		);

		self.id = new_buf_id;

		pending.push((
			EventAction::Request(shmp.wl_create_buffer(
				self.id,
				(self.offset, self.width, self.height, self.width * self.format.width()),
				self.format,
			)),
			WaylandObjectKind::Buffer,
			self.id,
		));

		Ok(pending)
	}
}

impl WaylandObject for Buffer {
	fn id(&self) -> Id {
		self.id
	}

	fn god(&self) -> WeRcGod {
		self.god.clone()
	}

	fn handle(
		&mut self,
		opcode: super::OpCode,
		_payload: &[u8],
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
			inv => return Err(WaylandError::InvalidOpCode(inv as OpCode, self.kind_as_str()).boxed()),
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
