use std::{collections::HashMap, error::Error, ffi::CString, fmt};

// std depends on libc anyway so i consider using it fair
// i may replace this with asm in the future but that means amd64 only
use libc::{O_CREAT, O_RDWR, ftruncate, shm_open, shm_unlink};

use crate::wayland::wire::{MessageManager, WireArgument, WireEvent, WireRequest};

pub mod wire;

pub struct Display {
	pub id: u32,
}

impl Display {
	pub fn new(wlim: &mut IdentManager) -> Self {
		Self {
			id: wlim.new_id_registered(WaylandObjectKind::Display),
		}
	}

	fn wl_get_registry(
		&mut self,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<u32, Box<dyn Error>> {
		let id = wlim.new_id_registered(WaylandObjectKind::Registry);
		wlmm.send_request(&mut WireRequest {
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

	pub fn wl_sync(
		&mut self,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<u32, Box<dyn Error>> {
		let cb_id = wlim.new_id_registered(WaylandObjectKind::Callback);
		wlmm.send_request(&mut WireRequest {
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
		wlim: &mut IdentManager,
	) -> Result<Self, Box<dyn Error>> {
		let reg_id = display.wl_get_registry(wlmm, wlim)?;
		let mut registry = Self::new_empty(reg_id);
		let cbid = display.wl_sync(wlmm, wlim)?;

		let mut events = vec![];
		let mut done = false;
		while !done {
			wlmm.get_events(wlim)?;

			while let Some(msg) = wlmm.q.pop_front() {
				if msg.recv_id == cbid {
					done = true;
					break;
				} else if msg.recv_id == registry.id {
					events.push(msg);
				}
			}
		}

		registry.fill(&events)?;
		Ok(registry)
	}

	fn wl_bind(
		&mut self,
		id: u32,
		object: WaylandObjectKind,
		version: u32,
		wlmm: &mut MessageManager,
	) -> Result<(), Box<dyn Error>> {
		let global_id = self
			.inner
			.iter()
			.find(|(_, v)| v.interface == object.as_str())
			.map(|(k, _)| k)
			.copied()
			.ok_or(WaylandError::NotInRegistry)?;
		println!("bind global id for {}: {}", object.as_str(), global_id);

		wlmm.send_request(&mut WireRequest {
			// wl_registry id
			sender_id: self.id,
			// first request in the proto
			opcode: 0,
			args: vec![
				WireArgument::UnInt(global_id),
				// WireArgument::NewId(new_id),
				WireArgument::NewIdSpecific(object.as_str(), version, id),
			],
		})?;
		Ok(())
	}

	pub fn fill(&mut self, events: &[WireEvent]) -> Result<(), Box<dyn Error>> {
		for e in events {
			if e.recv_id != self.id {
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

			self.inner.insert(
				name,
				RegistryEntry {
					interface,
					version,
				},
			);
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
		Self {
			id,
		}
	}

	pub fn new_bound(
		registry: &mut Registry,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<Self, Box<dyn Error>> {
		let id = wlim.new_id_registered(WaylandObjectKind::Compositor);
		registry.wl_bind(id, WaylandObjectKind::Compositor, 1, wlmm)?;
		Ok(Self::new(id))
	}

	fn wl_create_surface(
		&self,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<u32, Box<dyn Error>> {
		let id = wlim.new_id_registered(WaylandObjectKind::Surface);
		wlmm.send_request(&mut WireRequest {
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
		Self {
			id,
		}
	}

	pub fn new_bound(
		registry: &mut Registry,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<Self, Box<dyn Error>> {
		let id = wlim.new_id_registered(WaylandObjectKind::SharedMemory);
		registry.wl_bind(id, WaylandObjectKind::SharedMemory, 1, wlmm)?;
		Ok(Self::new(id))
	}

	fn wl_create_pool(
		&self,
		name: &CString,
		size: i32,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<(u32, i32), Box<dyn Error>> {
		let fd = unsafe { shm_open(name.as_ptr(), O_RDWR | O_CREAT, 0) };
		println!("fd: {}", fd);
		unsafe { ftruncate(fd, size.into()) };

		let id = wlim.new_id_registered(WaylandObjectKind::SharedMemoryPool);
		wlmm.send_request(&mut WireRequest {
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
		Self {
			id,
			name,
			size,
			fd,
		}
	}

	pub fn new_initialized(
		shm: &mut SharedMemory,
		size: i32,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
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
		wlim: &mut IdentManager,
	) -> Result<u32, Box<dyn Error>> {
		let id = wlim.new_id_registered(WaylandObjectKind::Buffer);
		wlmm.send_request(&mut WireRequest {
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
		wlmm.send_request(&mut WireRequest {
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
		wlim: &mut IdentManager,
	) -> Result<(), Box<dyn Error>> {
		self.wl_destroy(wlmm)?;
		wlim.free_id(self.id)?;
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
		wlim: &mut IdentManager,
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
		wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		})
	}

	pub fn destroy(
		&self,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<(), Box<dyn Error>> {
		self.wl_destroy(wlmm)?;
		wlim.free_id(self.id)?;
		Ok(())
	}
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum WaylandObjectKind {
	Display,
	Registry,
	Callback,
	Compositor,
	Surface,
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
			WaylandObjectKind::Surface => "wl_surface",
			WaylandObjectKind::SharedMemory => "wl_shm",
			WaylandObjectKind::SharedMemoryPool => "wl_shm_pool",
			WaylandObjectKind::Buffer => "wl_buffer",
			WaylandObjectKind::XdgWmBase => "xdg_wm_base",
		}
	}
}

#[derive(Default)]
pub struct IdentManager {
	top_id: u32,
	free: Vec<u32>,
	idmap: HashMap<u32, WaylandObjectKind>,
}

impl IdentManager {
	fn new_id(&mut self) -> u32 {
		self.top_id += 1;
		println!("! idman ! new id picked: {}", self.top_id);
		self.top_id
	}

	fn new_id_registered(&mut self, kind: WaylandObjectKind) -> u32 {
		let id = self.new_id();
		self.idmap.insert(id, kind);
		id
	}

	fn free_id(&mut self, id: u32) -> Result<(), Box<dyn Error>> {
		let registered = self.idmap.iter().find(|(k, _)| **k == id).map(|(k, _)| k).copied();
		if let Some(r) = registered {
			self.idmap.remove(&r).ok_or(WaylandError::IdMapRemovalFail.boxed())?;
		}
		self.free.push(id);
		Ok(())
	}
}

#[derive(Debug)]
pub enum WaylandError {
	ParseError,
	RecvLenBad,
	NotInRegistry,
	IdMapRemovalFail,
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
			WaylandError::IdMapRemovalFail => write!(f, "failed to remove from id man map"),
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
	pub id: u32,
}

impl XdgWmBase {
	pub fn new_bound(
		registry: &mut Registry,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<Self, Box<dyn Error>> {
		let id = wlim.new_id_registered(WaylandObjectKind::XdgWmBase);
		registry.wl_bind(id, WaylandObjectKind::XdgWmBase, 1, wlmm)?;
		Ok(Self {
			id,
		})
	}

	fn wl_destroy(&self, wlmm: &mut MessageManager) -> Result<(), Box<dyn Error>> {
		wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		})
	}

	fn wl_pong(&self, wlmm: &mut MessageManager) -> Result<(), Box<dyn Error>> {
		todo!()
	}

	pub fn destroy(
		&self,
		wlmm: &mut MessageManager,
		wlim: &mut IdentManager,
	) -> Result<(), Box<dyn Error>> {
		self.wl_destroy(wlmm)?;
		wlim.free_id(self.id)?;
		Ok(())
	}
}
