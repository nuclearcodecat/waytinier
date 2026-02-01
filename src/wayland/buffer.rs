use std::{cell::RefCell, error::Error, os::fd::OwnedFd, rc::Rc};

use crate::{
	NONE, WHITE, make_drop_impl,
	wayland::{
		DebugLevel, EventAction, ExpectRc, God, OpCode, RcCell, WaylandError, WaylandObject,
		WaylandObjectKind, WeRcGod, WeakCell,
		dmabuf::DmaBuf,
		shm::SharedMemoryPool,
		surface::Surface,
		wire::{Id, WireRequest},
	},
	wlog,
};

pub(crate) enum BufferBackend {
	SharedMemory(WeakCell<SharedMemoryPool>),
	Dma(WeakCell<DmaBuf>),
}

pub enum BufferBackendKind {
	SharedMemory,
	Dma,
}

impl From<&RcCell<SharedMemoryPool>> for BufferBackend {
	fn from(value: &RcCell<SharedMemoryPool>) -> Self {
		Self::SharedMemory(Rc::downgrade(value))
	}
}

impl From<&RcCell<DmaBuf>> for BufferBackend {
	fn from(value: &RcCell<DmaBuf>) -> Self {
		Self::Dma(Rc::downgrade(value))
	}
}

impl BufferBackend {
	pub(crate) fn make_buffer(
		&self,
		w: i32,
		h: i32,
		master: &RcCell<Surface>,
	) -> Result<RcCell<Buffer>, Box<dyn Error>> {
		let surface = master.borrow();
		match self {
			BufferBackend::SharedMemory(weak) => {
				let shmp = weak.upgrade().to_wl_err()?;
				let shmp = shmp.borrow_mut();
				let god = shmp.god().upgrade().to_wl_err()?;
				let mut god = god.borrow_mut();
				let buf = Rc::new(RefCell::new(Buffer {
					id: 0,
					god: shmp.god(),
					offset: 0,
					width: w,
					height: h,
					in_use: false,
					backend: Self::SharedMemory(weak.clone()),
					master: Rc::downgrade(master),
				}));
				let id = god.wlim.new_id_registered(WaylandObjectKind::Buffer, buf.clone());
				buf.borrow_mut().id = id;
				god.wlmm.queue_request(
					shmp.wl_create_buffer(id, (0, w, h, w * surface.pf.width()), surface.pf),
					buf.borrow().kind(),
				);
				Ok(buf)
			}
			BufferBackend::Dma(weak) => {
				let dmabuf = weak.upgrade().to_wl_err()?;
				let dmabuf = dmabuf.borrow_mut();
				let god = dmabuf.god().upgrade().to_wl_err()?;
				let mut god = god.borrow_mut();
				let buf = Rc::new(RefCell::new(Buffer {
					id: 0,
					god: dmabuf.god(),
					offset: 0,
					width: w,
					height: h,
					in_use: false,
					backend: Self::Dma(weak.clone()),
					master: Rc::downgrade(master),
				}));
				let id = god.wlim.new_id_registered(WaylandObjectKind::Buffer, buf.clone());
				buf.borrow_mut().id = id;
				todo!()
			}
		}
	}

	pub(crate) fn get_slice(&self) -> Result<*mut [u8], Box<dyn Error>> {
		match self {
			BufferBackend::SharedMemory(weak) => unsafe {
				Ok(&mut *weak.upgrade().to_wl_err()?.borrow_mut().slice.unwrap())
			},
			BufferBackend::Dma(weak) => todo!(),
		}
	}
}

pub(crate) struct Buffer {
	pub(crate) id: Id,
	pub(crate) god: WeRcGod,
	pub(crate) offset: i32,
	pub(crate) width: i32,
	pub(crate) height: i32,
	pub(crate) in_use: bool,
	pub(crate) backend: BufferBackend,
	pub(crate) master: WeakCell<Surface>,
}

impl Buffer {
	pub fn new_initalized(
		backend: BufferBackend,
		surface: &RcCell<Surface>,
		(offset, width, height): (i32, i32, i32),
		god: RcCell<God>,
	) -> Result<RcCell<Buffer>, Box<dyn Error>> {
		let buf = Rc::new(RefCell::new(Buffer {
			id: 0,
			god: Rc::downgrade(&god),
			offset,
			width,
			height,
			in_use: false,
			backend,
			master: Rc::downgrade(surface),
		}));
		let mut god = god.borrow_mut();
		let id = god.wlim.new_id_registered(WaylandObjectKind::Buffer, buf.clone());
		buf.borrow().backend.make_buffer(width, height, surface)?;
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
		self.queue_request(self.wl_destroy())?;
		god.wlim.free_id(self.id)?;
		Ok(())
	}

	#[allow(clippy::type_complexity)]
	pub(crate) fn get_resize_actions(
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

		match &self.backend {
			BufferBackend::SharedMemory(weak) => {
				let shmp = weak.upgrade().to_wl_err()?;
				let mut shmp = shmp.borrow_mut();
				let format = self.master.upgrade().to_wl_err()?.borrow().pf;
				let shm_actions = shmp.get_resize_actions_if_larger(w * h * format.width())?;
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
						(self.offset, self.width, self.height, self.width * format.width()),
						format,
					)),
					WaylandObjectKind::Buffer,
					self.id,
				));
			}
			BufferBackend::Dma(weak) => todo!(),
		};

		Ok(pending)
	}

	pub(crate) fn get_slice(&self) -> Result<*mut [u8], Box<dyn Error>> {
		self.backend.get_slice()
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

make_drop_impl!(Buffer, wl_destroy);
