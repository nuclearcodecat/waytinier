use crate::{
	CYAN, NONE, WHITE, dbug,
	linux::dma::fourcc_code,
	wayland::{
		DebugLevel, EventAction, ExpectRc, God, RcCell, WaylandError, WaylandObject,
		WaylandObjectKind, WeRcGod,
		registry::Registry,
		wire::{FromWireSingle, Id, WireArgument, WireRequest},
	},
	wlog,
};
use libc::{
	MAP_FAILED, MAP_SHARED, O_CREAT, O_RDWR, PROT_READ, PROT_WRITE, ftruncate, mmap, munmap,
	shm_open, shm_unlink,
};
use std::{
	cell::RefCell,
	collections::HashSet,
	error::Error,
	ffi::CString,
	os::{
		fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
		raw::c_void,
	},
	ptr::{self, null_mut},
	rc::Rc,
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

	pub const fn width(&self) -> i32 {
		match self {
			Self::Argb888 => 4,
			Self::Xrgb888 => 4,
		}
	}

	pub const fn to_fourcc(self) -> u32 {
		match self {
			PixelFormat::Argb888 => fourcc_code(b'X', b'R', b'2', b'4'),
			PixelFormat::Xrgb888 => fourcc_code(b'X', b'R', b'2', b'4'),
		}
	}

	pub const fn bpp(&self) -> u32 {
		match self {
			PixelFormat::Argb888 => 32,
			PixelFormat::Xrgb888 => 32,
		}
	}
}

pub struct SharedMemory {
	id: Id,
	god: WeRcGod,
	valid_pix_formats: HashSet<PixelFormat>,
}

impl SharedMemory {
	pub(crate) fn new(id: Id, god: WeRcGod) -> Self {
		Self {
			id,
			god,
			valid_pix_formats: HashSet::new(),
		}
	}

	fn push_pix_format(&mut self, pf: PixelFormat) {
		self.valid_pix_formats.insert(pf);
	}

	pub(crate) fn new_bound_initialized(
		registry: RcCell<Registry>,
		god: RcCell<God>,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let shm = Rc::new(RefCell::new(Self::new(0, Rc::downgrade(&god))));
		let id =
			god.borrow_mut().wlim.new_id_registered(WaylandObjectKind::SharedMemory, shm.clone());
		shm.borrow_mut().id = id;
		registry.borrow_mut().bind(id, WaylandObjectKind::SharedMemory, 1)?;
		Ok(shm)
	}

	fn make_unique_pool_name(&self) -> Result<CString, Box<dyn Error>> {
		let mut vec = vec![];
		while vec.len() < 16 {
			let random: u8 = std::random::random(..);
			if random > b'a' && random < b'z' {
				vec.push(random);
			}
		}
		let suffix = String::from_utf8_lossy(&vec);
		let name = format!("wl-shm-{}", suffix);
		dbug!(name);
		Ok(CString::new(name)?)
	}

	// call handle_events after!!
	pub(crate) fn make_pool(
		&mut self,
		size: i32,
	) -> Result<RcCell<SharedMemoryPool>, Box<dyn Error>> {
		let name = self.make_unique_pool_name()?;
		let raw_fd = unsafe { shm_open(name.as_ptr(), O_RDWR | O_CREAT, 0) };
		if raw_fd == -1 {
			return Err(Box::new(std::io::Error::last_os_error()));
		}
		wlog!(
			DebugLevel::Important,
			self.kind_as_str(),
			format!("new pool fd: {}", raw_fd),
			WHITE,
			NONE
		);
		if unsafe { ftruncate(raw_fd, size.into()) } == -1 {
			return Err(Box::new(std::io::Error::last_os_error()));
		}
		let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };

		let shmpool =
			Rc::new(RefCell::new(SharedMemoryPool::new(0, self.god.clone(), name, size, fd)));
		let id = self
			.god
			.upgrade()
			.to_wl_err()?
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::SharedMemoryPool, shmpool.clone());
		let shmpool_ = shmpool.clone();
		let mut shmpool_ = shmpool_.borrow_mut();
		shmpool_.id = id;
		shmpool_.update_ptr()?;
		self.create_pool(size, raw_fd, id)?;
		Ok(shmpool)
	}

	pub(crate) fn wl_create_pool(&self, size: i32, fd: RawFd, id: Id) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![
				// WireArgument::NewIdSpecific(WaylandObjectKind::SharedMemoryPool.as_str(), 1, id),
				WireArgument::NewId(id),
				WireArgument::FileDescriptor(fd),
				WireArgument::Int(size),
			],
		}
	}

	pub(crate) fn create_pool(&self, size: i32, fd: RawFd, id: Id) -> Result<(), Box<dyn Error>> {
		self.queue_request(self.wl_create_pool(size, fd, id))
	}
}

pub struct SharedMemoryPool {
	pub(crate) id: Id,
	god: WeRcGod,
	name: CString,
	pub size: i32,
	pub(crate) fd: OwnedFd,
	pub slice: Option<*mut [u8]>,
	ptr: Option<*mut c_void>,
}

impl SharedMemoryPool {
	pub fn new(id: Id, god: WeRcGod, name: CString, size: i32, fd: OwnedFd) -> Self {
		Self {
			id,
			god,
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

	fn unmap(&self) -> Result<(), Box<dyn Error>> {
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

	pub(crate) fn destroy(&self) -> Result<(), Box<dyn Error>> {
		self.queue_request(self.wl_destroy())?;
		self.unmap()?;
		self.unlink()?;
		Ok(())
	}

	pub(crate) fn update_ptr(&mut self) -> Result<(), Box<dyn Error>> {
		let ptr = unsafe {
			mmap(
				null_mut(),
				self.size as usize,
				PROT_READ | PROT_WRITE,
				MAP_SHARED,
				self.fd.as_raw_fd(),
				0,
			)
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

	pub(crate) fn get_resize_actions_if_larger(
		&mut self,
		size: i32,
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		if size < self.size {
			return Ok(pending);
		}
		pending.push(EventAction::DebugMessage(
			DebugLevel::Important,
			format!("{} | RESIZE size {size}", self.kind_as_str()),
		));
		self.unmap()?;
		self.size = size;
		let r = unsafe { ftruncate(self.fd.as_raw_fd(), size.into()) };
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
	fn id(&self) -> Id {
		self.id
	}

	fn god(&self) -> WeRcGod {
		self.god.clone()
	}

	fn handle(
		&mut self,
		opcode: super::OpCode,
		payload: &[u8],
		_fds: &[OwnedFd],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			0 => {
				let format = u32::from_wire_element(payload)?;
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
				return Err(WaylandError::InvalidOpCode(inv, self.kind_as_str()).boxed());
			}
		}
		Ok(pending)
	}

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::SharedMemory
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}

impl WaylandObject for SharedMemoryPool {
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

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::SharedMemoryPool
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}

impl Drop for SharedMemoryPool {
	fn drop(&mut self) {
		// todo remove this unwrap
		wlog!(DebugLevel::Important, self.kind_as_str(), "dropping self", WHITE, CYAN);
		self.destroy().unwrap();
	}
}
