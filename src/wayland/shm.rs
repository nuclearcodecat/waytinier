use std::{
	cell::RefCell,
	collections::HashSet,
	error::Error,
	ffi::CString,
	os::{fd::RawFd, raw::c_void},
	ptr::{self, null_mut},
	rc::Rc,
};
// std depends on libc anyway so i consider using it fair
// i may replace this with asm in the future but that means amd64 only
use crate::wayland::{
	Context, CtxType, DebugLevel, EventAction, ExpectRc, RcCell, WaylandError, WaylandObject, WaylandObjectKind, registry::Registry, wire::{FromWirePayload, Id, WireArgument, WireRequest}
};
use libc::{
	MAP_FAILED, MAP_SHARED, O_CREAT, O_RDWR, PROT_READ, PROT_WRITE, close, ftruncate, mmap, munmap,
	shm_open, shm_unlink,
};

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PixelFormat {
	Argb888,
	Xrgb888,
}

impl PixelFormat {
	pub(crate) fn from_u32(processee: u32) -> Result<PixelFormat, Box<dyn Error>> {
		match processee {
			0 => Ok(PixelFormat::Argb888),
			1 => Ok(PixelFormat::Xrgb888),
			_ => Err(WaylandError::InvalidPixelFormat.boxed()),
		}
	}

	pub fn width(&self) -> usize {
		match self {
			Self::Argb888 => 4,
			Self::Xrgb888 => 4,
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
		ctx: RcCell<Context>,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let shm = Rc::new(RefCell::new(Self::new(0, Rc::downgrade(&ctx))));
		let id =
			ctx.borrow_mut().wlim.new_id_registered(WaylandObjectKind::SharedMemory, shm.clone());
		shm.borrow_mut().id = id;
		registry.bind(id, WaylandObjectKind::SharedMemory, 1)?;
		Ok(shm)
	}

	pub fn make_pool(&mut self, size: i32) -> Result<RcCell<SharedMemoryPool>, Box<dyn Error>> {
		// add method to get new names
		let name = CString::new("wl-shm-1")?;
		let fd = unsafe { shm_open(name.as_ptr(), O_RDWR | O_CREAT, 0) };
		if fd == -1 {
			return Err(Box::new(std::io::Error::last_os_error()));
		}
		println!("fd: {}", fd);
		if unsafe { ftruncate(fd, size.into()) } == -1 {
			return Err(Box::new(std::io::Error::last_os_error()));
		}

		let shmpool =
			Rc::new(RefCell::new(SharedMemoryPool::new(0, self.ctx.clone(), name, size, fd)));
		let id = self
			.ctx
			.upgrade()
			.to_wl_err()?
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::SharedMemoryPool, shmpool.clone());
		let shmpool_ = shmpool.clone();
		let mut shmpool_ = shmpool_.borrow_mut();
		shmpool_.id = id;
		shmpool_.update_ptr()?;
		self.wl_create_pool(size, fd, id)?;
		Ok(shmpool)
	}

	pub(crate) fn wl_create_pool(
		&self,
		size: i32,
		fd: RawFd,
		id: Id,
	) -> Result<(), Box<dyn Error>> {
		self.ctx.upgrade().to_wl_err()?.borrow().wlmm.send_request(&mut WireRequest {
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
	pub size: i32,
	pub(crate) fd: RawFd,
	pub slice: Option<*mut [u8]>,
	ptr: Option<*mut c_void>,
}

impl SharedMemoryPool {
	pub fn new(id: Id, ctx: CtxType, name: CString, size: i32, fd: RawFd) -> Self {
		Self {
			id,
			ctx,
			name,
			size,
			fd,
			slice: None,
			ptr: None,
		}
	}

	pub(crate) fn wl_create_buffer(
		&self,
		id: Id,
		(offset, width, height, stride): (i32, i32, i32, i32),
		format: PixelFormat,
	) -> WireRequest {
		WireRequest {
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
		}
	}

	pub(crate) fn wl_destroy(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![],
		}
	}

	pub(crate) fn wl_resize(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 2,
			args: vec![WireArgument::Int(self.size)],
		}
	}

	pub(crate) fn unmap(&self) -> Result<(), Box<dyn Error>> {
		if let Some(ptr) = self.ptr {
			if unsafe { munmap(ptr, self.size as usize) } == 0 {
				Ok(())
			} else {
				Err(Box::new(std::io::Error::last_os_error()))
			}
		} else {
			Err(WaylandError::RequiredValueNone.boxed())
		}
	}

	fn unlink(&self) -> Result<(), std::io::Error> {
		let r = unsafe { shm_unlink(self.name.as_ptr()) };
		if r == 0 {
			Ok(())
		} else {
			Err(std::io::Error::last_os_error())
		}
	}

	fn close(&self) -> Result<(), std::io::Error> {
		let r = unsafe { close(self.fd) };
		if r == 0 {
			Ok(())
		} else {
			Err(std::io::Error::last_os_error())
		}
	}

	pub fn destroy(&self) -> Result<(), Box<dyn Error>> {
		let ctx = self.ctx.upgrade().to_wl_err()?;
		let mut ctx = ctx.borrow_mut();
		ctx.wlmm.send_request(&mut self.wl_destroy())?;
		ctx.wlim.free_id(self.id)?;
		self.unmap()?;
		self.unlink()?;
		self.close()?;
		Ok(())
	}

	pub(crate) fn update_ptr(&mut self) -> Result<(), Box<dyn Error>> {
		let ptr = unsafe {
			mmap(null_mut(), self.size as usize, PROT_READ | PROT_WRITE, MAP_SHARED, self.fd, 0)
		};
		if ptr == MAP_FAILED {
			eprintln!("FAILED IN UPDATE_PTR");
			return Err(Box::new(std::io::Error::last_os_error()));
		}

		let x: *mut [u8] = ptr::slice_from_raw_parts_mut(ptr as *mut u8, self.size as usize);
		self.ptr = Some(ptr);
		self.slice = Some(x);
		Ok(())
	}

	pub(crate) fn resize_if_larger(
		&mut self,
		size: i32,
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		if size < self.size {
			return Ok(pending);
		}
		pending.push(EventAction::DebugMessage(DebugLevel::Important, format!("shm pool | RESIZE size {size}")));
		self.unmap()?;
		self.size = size;
		let r = unsafe { ftruncate(self.fd, size.into()) };
		if r == 0 {
			Ok(())
		} else {
			Err(std::io::Error::last_os_error())
		}?;
		self.update_ptr()?;
		pending.push(EventAction::Request(self.wl_resize()));
		Ok(pending)
	}
}

impl WaylandObject for SharedMemory {
	fn handle(
		&mut self,
		opcode: super::OpCode,
		payload: &[u8],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			0 => {
				let format = u32::from_wire(payload)?;
				if let Ok(pf) = PixelFormat::from_u32(format) {
					self.push_pix_format(pf);
					pending.push(EventAction::DebugMessage(
						crate::wayland::DebugLevel::Trivial,
						format!("pushing pixel format {:?} (0x{:08x})", pf, format),
					));
				} else {
					pending.push(EventAction::DebugMessage(
						crate::wayland::DebugLevel::Error,
						format!("found unrecognized pixel format 0x{:08x}", format),
					));
				}
			}
			inv => {
				return Err(WaylandError::InvalidOpCode(inv, self.as_str()).boxed());
			}
		}
		Ok(pending)
	}

	fn as_str(&self) -> &'static str {
		WaylandObjectKind::SharedMemory.as_str()
	}
}

impl WaylandObject for SharedMemoryPool {
	fn handle(
		&mut self,
		_opcode: super::OpCode,
		_payload: &[u8],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		todo!()
	}

	fn as_str(&self) -> &'static str {
		WaylandObjectKind::SharedMemoryPool.as_str()
	}
}
