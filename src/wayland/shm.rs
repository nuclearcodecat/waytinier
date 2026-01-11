use std::{cell::RefCell, collections::HashSet, error::Error, ffi::CString, os::fd::RawFd, rc::Rc};
// std depends on libc anyway so i consider using it fair
// i may replace this with asm in the future but that means amd64 only
use crate::wayland::{
	CtxType, RcCell, WaylandError, WaylandObject, WaylandObjectKind,
	buffer::Buffer,
	registry::Registry,
	wire::{FromWirePayload, Id, WireArgument, WireRequest},
};
use libc::{O_CREAT, O_RDWR, ftruncate, shm_open, shm_unlink};

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PixelFormat {
	Argb888,
	Xrgb888,
}

impl PixelFormat {
	pub fn from_u32(processee: u32) -> Result<PixelFormat, Box<dyn Error>> {
		match processee {
			0 => Ok(PixelFormat::Argb888),
			1 => Ok(PixelFormat::Xrgb888),
			_ => Err(WaylandError::InvalidPixelFormat.boxed()),
		}
	}
}

pub struct SharedMemory {
	id: Id,
	ctx: CtxType,
	valid_pix_formats: HashSet<PixelFormat>,
}

impl SharedMemory {
	pub fn new(id: Id, ctx: CtxType) -> Self {
		Self {
			id,
			ctx,
			valid_pix_formats: HashSet::new(),
		}
	}

	fn push_pix_format(&mut self, pf: PixelFormat) {
		self.valid_pix_formats.insert(pf);
	}

	pub fn new_bound_initialized(
		registry: &mut Registry,
		ctx: CtxType,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let shm = Rc::new(RefCell::new(Self::new(0, ctx.clone())));
		let id =
			ctx.borrow_mut().wlim.new_id_registered(WaylandObjectKind::SharedMemory, shm.clone());
		shm.borrow_mut().id = id;
		registry.bind(id, WaylandObjectKind::SharedMemory, 1)?;
		Ok(shm)
	}

	pub fn make_pool(&self, size: i32) -> Result<RcCell<SharedMemoryPool>, Box<dyn Error>> {
		// add method to get new names
		let name = CString::new("wl-shm-1")?;
		let fd = unsafe { shm_open(name.as_ptr(), O_RDWR | O_CREAT, 0) };
		println!("fd: {}", fd);
		unsafe { ftruncate(fd, size.into()) };

		let shmpool =
			Rc::new(RefCell::new(SharedMemoryPool::new(0, self.ctx.clone(), name, fd, size)));
		let id = self
			.ctx
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::SharedMemoryPool, shmpool.clone());
		shmpool.borrow_mut().id = id;
		self.wl_create_pool(size, fd, id)?;
		Ok(shmpool)
	}

	pub(crate) fn wl_create_pool(
		&self,
		size: i32,
		fd: RawFd,
		id: Id,
	) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![
				// WireArgument::NewIdSpecific(WaylandObjectKind::SharedMemoryPool.as_str(), 1, id),
				WireArgument::NewId(id),
				WireArgument::FileDescriptor(fd),
				WireArgument::Int(size),
			],
		})
	}
}

pub struct SharedMemoryPool {
	id: Id,
	ctx: CtxType,
	name: CString,
	size: i32,
	fd: RawFd,
}

impl SharedMemoryPool {
	pub fn new(id: Id, ctx: CtxType, name: CString, size: i32, fd: RawFd) -> Self {
		Self {
			id,
			ctx,
			name,
			size,
			fd,
		}
	}

	pub(crate) fn wl_create_buffer(
		&self,
		id: Id,
		(offset, width, height, stride): (i32, i32, i32, i32),
		format: PixelFormat,
	) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![
				WireArgument::NewId(id),
				WireArgument::Int(offset),
				WireArgument::Int(width),
				WireArgument::Int(height),
				WireArgument::Int(stride),
				WireArgument::UnInt(format as u32),
			],
		})
	}

	pub fn make_buffer(
		&self,
		(offset, width, height, stride): (i32, i32, i32, i32),
		format: PixelFormat,
	) -> Result<RcCell<Buffer>, Box<dyn Error>> {
		let buf = Rc::new(RefCell::new(Buffer {
			id: 0,
			ctx: self.ctx.clone(),
			offset,
			width,
			height,
			stride,
			format,
		}));
		let id =
			self.ctx.borrow_mut().wlim.new_id_registered(WaylandObjectKind::Buffer, buf.clone());
		buf.borrow_mut().id = id;
		self.wl_create_buffer(id, (offset, width, height, stride), format)?;
		Ok(buf)
	}

	pub(crate) fn wl_destroy(&self) -> Result<(), Box<dyn Error>> {
		self.ctx.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![],
		})
	}

	fn unlink(&self) -> Result<(), std::io::Error> {
		let r = unsafe { shm_unlink(self.name.as_ptr()) };
		if r == 0 {
			Ok(())
		} else {
			Err(std::io::Error::last_os_error())
		}
	}

	pub fn destroy(&self) -> Result<(), Box<dyn Error>> {
		self.wl_destroy()?;
		self.ctx.borrow_mut().wlim.free_id(self.id)?;
		Ok(self.unlink()?)
	}
}

impl WaylandObject for SharedMemory {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		match opcode {
			0 => {
				let format = u32::from_wire(&payload[8..])?;
				if let Ok(pf) = PixelFormat::from_u32(format) {
					self.push_pix_format(pf);
				} else {
					eprintln!("found unrecognized pixel format 0x{:08x}", format);
				}
			}
			inv => {
				eprintln!("invalid shm opcode {}", inv);
			}
		}
		Ok(())
	}
}

impl WaylandObject for SharedMemoryPool {
	fn handle(&mut self, opcode: super::OpCode, payload: &[u8]) -> Result<(), Box<dyn Error>> {
		todo!()
	}
}
