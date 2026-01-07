use std::{collections::HashMap, error::Error, ffi::CString, fmt};

// std depends on libc anyway so i consider using it fair
// i may replace this with asm in the future but that means amd64 only
use libc::{O_CREAT, O_RDWR, ftruncate, shm_open, shm_unlink};

use crate::wayland::wire::{MessageManager, WireArgument, WireMessage};

pub mod wire;

pub struct Display {
	pub id: u32,
}

impl Display {
	pub fn new(wlim: &mut IdManager) -> Self {
		Self { id: wlim.new_id() }
	}

	fn wl_get_registry(
		&mut self,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<u32, Box<dyn Error>> {
		let id = wlim.new_id();
		wlmm.send_request(&mut WireMessage {
			sender_id: self.id,
			// second request in the proto
			opcode: 1,
			args: vec![
				// wl_registry id is now 2 since 1 is the display
				WireArgument::NewId(id),
			],
		})?;
		Ok(id)
	}

	fn wl_sync(
		&mut self,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<u32, Box<dyn Error>> {
		let cb_id = wlim.new_id();
		wlmm.send_request(&mut WireMessage {
			sender_id: self.id,
			opcode: 0,
			args: vec![WireArgument::NewId(cb_id)],
		})?;
		Ok(cb_id)
	}
}

pub struct Registry {
	id: u32,
	inner: HashMap<u32, RegistryEntry>,
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct RegistryEntry {
	interface: String,
	version: u32,
}

impl Registry {
	pub fn new_empty(id: u32) -> Self {
		Self {
			id,
			inner: HashMap::new(),
		}
	}

	pub fn new_bound_filled(
		display: &mut Display,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<Self, Box<dyn Error>> {
		let reg_id = display.wl_get_registry(wlmm, wlim)?;
		let mut registry = Self::new_empty(reg_id);

		let read = wlmm.get_events_blocking(registry.id, WaylandObjectKind::Registry)?;
		registry.fill(&read)?;
		Ok(registry)
	}

	fn wl_bind(
		&mut self,
		object: WaylandObjectKind,
		version: u32,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<u32, Box<dyn Error>> {
		let global_id = self
			.inner
			.iter()
			.find(|(_, v)| v.interface == object.as_str())
			.map(|(k, _)| k)
			.copied()
			.ok_or(WaylandError::NotInRegistry)?;
		println!("bind global id for {}: {}", object.as_str(), global_id);
		let new_id = wlim.new_id();

		wlmm.send_request(&mut WireMessage {
			// wl_registry id
			sender_id: self.id,
			// first request in the proto
			opcode: 0,
			args: vec![
				WireArgument::UnInt(global_id),
				// WireArgument::NewId(new_id),
				WireArgument::NewIdSpecific(object.as_str(), version, new_id),
			],
		})?;

		Ok(new_id)
	}

	pub fn fill(&mut self, events: &[WireMessage]) -> Result<(), Box<dyn Error>> {
		for e in events {
			if e.sender_id != self.id {
				continue;
			};
			let name;
			let interface;
			let version;
			if let WireArgument::UnInt(name_) = e.args[0] {
				name = name_;
			} else {
				return Err(WaylandError::ParseError.boxed());
			};
			if let WireArgument::String(interface_) = &e.args[1] {
				interface = interface_.clone();
			} else {
				return Err(WaylandError::ParseError.boxed());
			};
			if let WireArgument::UnInt(version_) = e.args[2] {
				version = version_;
			} else {
				return Err(WaylandError::ParseError.boxed());
			};

			self.inner
				.insert(name, RegistryEntry { interface, version });
		}
		Ok(())
	}

	pub fn does_implement(&self, query: &str) -> Option<u32> {
		self.inner.iter().find(|(_, v)| v.interface == query).map(|(_, v)| v.version)
	}
}

pub struct Compositor {
	pub id: u32,
}

impl Compositor {
	pub fn new(id: u32) -> Self {
		Self { id }
	}

	pub fn new_bound(
		registry: &mut Registry,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<Self, Box<dyn Error>> {
		let id = registry.wl_bind(WaylandObjectKind::Compositor, 1, wlmm, wlim)?;
		Ok(Self::new(id))
	}

	fn wl_create_surface(
		&self,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<u32, Box<dyn Error>> {
		let id = wlim.new_id();
		wlmm.send_request(&mut WireMessage {
			sender_id: self.id,
			opcode: 0,
			args: vec![WireArgument::UnInt(id)],
		})?;
		Ok(id)
	}
}

pub struct SharedMemory {
	id: u32,
}

impl SharedMemory {
	pub fn new(id: u32) -> Self {
		Self { id }
	}

	pub fn new_bound(
		registry: &mut Registry,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<Self, Box<dyn Error>> {
		let id = registry.wl_bind(WaylandObjectKind::SharedMemory, 1, wlmm, wlim)?;
		Ok(Self::new(id))
	}

	fn wl_create_pool(
		&self,
		name: &CString,
		size: i32,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<(u32, i32), Box<dyn Error>> {
		let fd = unsafe { shm_open(name.as_ptr(), O_RDWR | O_CREAT, 0) };
		println!("fd: {}", fd);
		unsafe { ftruncate(fd, size.into()) };

		let id = wlim.new_id();
		wlmm.send_request(&mut WireMessage {
			sender_id: self.id,
			opcode: 0,
			args: vec![
				// WireArgument::NewIdSpecific(WaylandObjectKind::SharedMemoryPool.as_str(), 1, id),
				WireArgument::NewId(id),
				WireArgument::FileDescriptor(fd),
				WireArgument::Int(size),
			],
		})?;
		Ok((id, fd))
	}
}

pub struct SharedMemoryPool {
	id: u32,
	name: CString,
	size: i32,
	fd: i32,
}

impl SharedMemoryPool {
	pub fn new(id: u32, name: CString, size: i32, fd: i32) -> Self {
		Self { id, name, size, fd }
	}

	pub fn new_initialized(
		shm: &mut SharedMemory,
		size: i32,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<Self, Box<dyn Error>> {
		let name = CString::new("wl-shm-1")?;
		let (id, fd) = shm.wl_create_pool(&name, size, wlmm, wlim)?;
		Ok(Self::new(id, name, size, fd))
	}

	fn wl_create_buffer(
		&self,
		(offset, width, height, stride): (i32, i32, i32, i32),
		format: PixelFormat,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<u32, Box<dyn Error>> {
		let id = wlim.new_id();
		wlmm.send_request(&mut WireMessage {
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
		})?;
		Ok(id)
	}

	fn wl_destroy(&self, wlmm: &mut MessageManager) -> Result<(), Box<dyn Error>> {
		wlmm.send_request(&mut WireMessage {
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

	pub fn destroy(
		&self,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<(), Box<dyn Error>> {
		self.wl_destroy(wlmm)?;
		wlim.free_id(self.id);
		Ok(self.unlink()?)
	}
}

pub struct Buffer {
	id: u32,
	pub offset: i32,
	pub width: i32,
	pub height: i32,
	pub stride: i32,
	pub format: PixelFormat,
}

impl Buffer {
	pub fn new_initialized(
		shm_pool: &mut SharedMemoryPool,
		(offset, width, height, stride): (i32, i32, i32, i32),
		format: PixelFormat,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<Self, Box<dyn Error>> {
		let id = shm_pool.wl_create_buffer((offset, width, height, stride), format, wlmm, wlim)?;
		Ok(Self {
			id,
			offset,
			width,
			height,
			stride,
			format,
		})
	}

	fn wl_destroy(&self, wlmm: &mut MessageManager) -> Result<(), Box<dyn Error>> {
		wlmm.send_request(&mut WireMessage {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		})
	}

	pub fn destroy(
		&self,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<(), Box<dyn Error>> {
		self.wl_destroy(wlmm)?;
		wlim.free_id(self.id);
		Ok(())
	}
}

#[derive(PartialEq)]
pub enum WaylandObjectKind {
	Display,
	Registry,
	Callback,
	Compositor,
	SharedMemory,
	SharedMemoryPool,
	Buffer,
	XdgWmBase,
}

impl WaylandObjectKind {
	fn as_str(&self) -> &'static str {
		match self {
			WaylandObjectKind::Display => "wl_display",
			WaylandObjectKind::Registry => "wl_registry",
			WaylandObjectKind::Callback => "wl_callback",
			WaylandObjectKind::Compositor => "wl_compositor",
			WaylandObjectKind::SharedMemory => "wl_shm",
			WaylandObjectKind::SharedMemoryPool => "wl_shm_pool",
			WaylandObjectKind::Buffer => "wl_buffer",
			WaylandObjectKind::XdgWmBase => "xdg_wm_base",
		}
	}
}

#[derive(Default)]
pub struct IdManager {
	top_id: u32,
	free: Vec<u32>,
}

impl IdManager {
	fn new_id(&mut self) -> u32 {
		self.top_id += 1;
		println!("! idman ! new id picked: {}", self.top_id);
		self.top_id
	}

	fn free_id(&mut self, id: u32) {
		self.free.push(id);
	}
}

#[derive(Debug)]
pub enum WaylandError {
	ParseError,
	RecvLenBad,
	NotInRegistry,
	ObjectNonExistent,
}

impl WaylandError {
	fn boxed(self) -> Box<Self> {
		Box::new(self)
	}
}

impl fmt::Display for WaylandError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			WaylandError::ParseError => write!(f, "parse error"),
			WaylandError::RecvLenBad => write!(f, "received len is bad"),
			WaylandError::NotInRegistry => {
				write!(f, "given name was not found in the registry hashmap")
			}
			WaylandError::ObjectNonExistent => write!(f, "requested object doesn't exist"),
		}
	}
}

impl Error for WaylandError {}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum PixelFormat {
	Argb888,
	Xrgb888,
}

pub struct XdgWmBase {
	id: u32,
}

impl XdgWmBase {
	pub fn new_bound(
		registry: &mut Registry,
		wlmm: &mut MessageManager,
		wlim: &mut IdManager,
	) -> Result<Self, Box<dyn Error>> {
		let id = registry.wl_bind(WaylandObjectKind::XdgWmBase, 1, wlmm, wlim)?;
		Ok(Self { id })
	}

	fn wl_destroy(&self, wlmm: &mut MessageManager) -> Result<(), Box<dyn Error>> {
		wlmm.send_request(&mut WireMessage {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		})
	}

	pub fn destroy(&self, wlmm: &mut MessageManager, wlim: &mut IdManager) -> Result<(), Box<dyn Error>> {
		self.wl_destroy(wlmm)?;
		wlim.free_id(self.id);
		Ok(())
	}
}
