use std::{cell::RefCell, error::Error, rc::Rc};

use crate::wayland::{
	CtxType, DebugLevel, EventAction, OpCode, RcCell, WaylandError, WaylandObject,
	WaylandObjectKind,
	shm::{PixelFormat, SharedMemoryPool},
	wire::{Id, WireRequest},
};

pub struct Buffer {
	pub id: Id,
	pub(crate) ctx: CtxType,
	pub offset: i32,
	pub width: i32,
	pub height: i32,
	pub format: PixelFormat,
	pub in_use: bool,
	pub shm_pool: RcCell<SharedMemoryPool>,
}

impl Buffer {
	pub fn new_initalized(
		shmp: RcCell<SharedMemoryPool>,
		(offset, width, height): (i32, i32, i32),
		format: PixelFormat,
		ctx: CtxType,
	) -> Result<RcCell<Buffer>, Box<dyn Error>> {
		let buf = Rc::new(RefCell::new(Buffer {
			id: 0,
			ctx: ctx.clone(),
			offset,
			width,
			height,
			format,
			in_use: false,
			shm_pool: shmp.clone(),
		}));
		let id = ctx.borrow_mut().wlim.new_id_registered(WaylandObjectKind::Buffer, buf.clone());
		buf.borrow_mut().id = id;
		ctx.borrow().wlmm.send_request(&mut shmp.borrow().wl_create_buffer(
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
		self.ctx.borrow().wlmm.send_request(&mut self.wl_destroy())?;
		self.ctx.borrow_mut().wlim.free_id(self.id)?;
		Ok(())
	}

	pub(crate) fn resize(&mut self, new_buf_id: Id, (w, h): (i32, i32)) -> Result<Vec<WireRequest>, Box<dyn Error>> {
		let mut pending = vec![];
		self.width = w;
		self.height = h;
		println!("! buffer ! RESIZE w: {} h: {}", self.width, self.height);

		pending.push(self.wl_destroy());

		let shmp = self.shm_pool.clone();
		let mut shmp = shmp.borrow_mut();
		let mut shm_actions = shmp.resize_if_larger(w * h * self.format.width() as i32)?;
		pending.append(&mut shm_actions);

		self.id = new_buf_id;

		pending.push(shmp.wl_create_buffer(
			self.id,
			(self.offset, self.width, self.height, self.width * self.format.width() as i32),
			self.format,
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
					DebugLevel::Verbose,
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
