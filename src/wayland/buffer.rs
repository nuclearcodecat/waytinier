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
	) -> Result<RcCell<Buffer>, Box<dyn Error>> {
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
		buf.borrow_mut().id = id;
		god.wlmm.send_request(&mut shmp.borrow().wl_create_buffer(
			id,
			(offset, width, height, width * format.width() as i32),
			format,
		))?;
		Ok(buf)
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
		god.wlmm.send_request(&mut self.wl_destroy())?;
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
			self.as_str(),
			format!("RESIZE w: {} h: {}", self.width, self.height),
			WHITE,
			NONE
		);

		pending.push((EventAction::Request(self.wl_destroy()), WaylandObjectKind::Buffer, self.id));

		let shmp = self.shm_pool.upgrade().to_wl_err()?;
		let mut shmp = shmp.borrow_mut();
		let shm_actions = shmp.resize_if_larger(w * h * self.format.width() as i32)?;
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
				(self.offset, self.width, self.height, self.width * self.format.width() as i32),
				self.format,
			)),
			WaylandObjectKind::Buffer,
			self.id,
		));

		Ok(pending)
	}
}

impl WaylandObject for Buffer {
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
					format!("{} not in use anymore", self.as_str()),
				))
			}
			inv => return Err(WaylandError::InvalidOpCode(inv as OpCode, self.as_str()).boxed()),
		};
		Ok(pending)
	}

	fn as_str(&self) -> &'static str {
		WaylandObjectKind::Buffer.as_str()
	}
}
