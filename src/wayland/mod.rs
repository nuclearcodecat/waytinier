use crate::wayland::{
	wire::{Id, MessageManager, WireRequest},
	xdgshell::XdgSurface,
};
use std::{
	cell::RefCell,
	collections::HashMap,
	error::Error,
	fmt::{self, Display},
	rc::Rc,
};
pub mod buffer;
pub mod callback;
pub mod compositor;
pub mod display;
pub mod region;
pub mod registry;
pub mod shm;
pub mod surface;
pub mod wire;
pub mod xdgshell;

pub type OpCode = usize;

#[derive(Debug)]
struct RecvError {
	id: Id,
	code: u32,
	msg: String,
}

impl Display for RecvError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "id: {}, code: {}\nmsg: {}", self.id, self.code, self.msg)
	}
}

impl Error for RecvError {}

impl RecvError {
	pub fn boxed(self) -> Box<Self> {
		Box::new(self)
	}
}

#[repr(usize)]
pub(crate) enum DebugLevel {
	Verbose,
	Important,
	Severe,
}

pub(crate) enum EventAction {
	Request(WireRequest),
	IdDeletion(Id),
	Error(Box<dyn Error>),
	DebugMessage(DebugLevel, String),
	Resize(i32, i32),
}

pub(crate) trait WaylandObject {
	fn handle(
		&mut self,
		opcode: OpCode,
		payload: &[u8],
	) -> Result<Vec<EventAction>, Box<dyn Error>>;
	fn as_str(&self) -> &'static str;
}

pub type CtxType = Rc<RefCell<Context>>;

pub struct Context {
	wlmm: MessageManager,
	wlim: IdentManager,
	xdg_surface: Option<RcCell<XdgSurface>>,
}

impl Context {
	pub fn new(wlmm: MessageManager, wlim: IdentManager) -> Self {
		Self {
			wlmm,
			wlim,
			xdg_surface: None,
		}
	}

	pub fn handle_events(&mut self) -> Result<(), Box<dyn Error>> {
		let mut retries = 0;
		while self.wlmm.get_events()? == 0 && retries < 9999 {
			retries += 1;
		}
		let mut actions: Vec<EventAction> = vec![];
		while let Some(ev) = self.wlmm.q.pop_front() {
			let obj = self.wlim.find_obj_by_id(ev.recv_id)?;
			println!("going to handle {:?}", obj.0);
			let mut x = obj.1.borrow_mut().handle(ev.opcode, &ev.payload)?;
			actions.append(&mut x);
		}
		for act in actions {
			match act {
				EventAction::Request(mut msg) => {
					self.wlmm.send_request(&mut msg)?;
				}
				EventAction::IdDeletion(id) => {
					self.wlim.free_id(id)?;
				}
				// add colors
				EventAction::Error(er) => eprintln!("{:?}", er),
				// add colors
				EventAction::DebugMessage(_, msg) => println!("{msg}"),
				EventAction::Resize(w, h) => {
					let xdgs = self.xdg_surface.clone().ok_or(WaylandError::ObjectNonExistent)?;
					let xdgs = xdgs.borrow_mut();
					let surf = xdgs.wl_surface.borrow_mut();
					let buf = surf.attached_buf.clone().ok_or(WaylandError::ObjectNonExistent)?;
					let mut buf = buf.borrow_mut();
					buf.resize((w, h))?;
				}
			};
		}
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
	XdgSurface,
	XdgTopLevel,
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
			WaylandObjectKind::XdgSurface => "xdg_surface",
			WaylandObjectKind::XdgTopLevel => "xdg_toplevel",
		}
	}
}

// wayland trait object
type Wlto = Rc<RefCell<dyn WaylandObject>>;
pub type RcCell<T> = Rc<RefCell<T>>;

#[derive(Default)]
pub struct IdentManager {
	top_id: Id,
	free: Vec<Id>,
	idmap: HashMap<Id, (WaylandObjectKind, Wlto)>,
}

impl IdentManager {
	pub(crate) fn new_id(&mut self) -> Id {
		self.top_id += 1;
		println!("! idman ! new id picked: {}", self.top_id);
		self.top_id
	}

	pub(crate) fn new_id_registered(&mut self, kind: WaylandObjectKind, obj: Wlto) -> Id {
		let id = self.new_id();
		self.idmap.insert(id, (kind, obj));
		id
	}

	pub(crate) fn free_id(&mut self, id: Id) -> Result<(), Box<dyn Error>> {
		let registered = self.idmap.iter().find(|(k, _)| **k == id).map(|(k, _)| k).copied();
		if let Some(r) = registered {
			self.idmap.remove(&r).ok_or(WaylandError::IdMapRemovalFail.boxed())?;
		}
		self.free.push(id);
		Ok(())
	}

	// ugh
	pub(crate) fn find_obj_by_id(
		&self,
		id: Id,
	) -> Result<&(WaylandObjectKind, Wlto), WaylandError> {
		self.idmap
			.iter()
			.find(|(k, _)| **k == id)
			.map(|(_, v)| v)
			.ok_or_else(|| WaylandError::ObjectNonExistent)
	}

	pub(crate) fn find_obj_kind_by_id(&self, id: Id) -> Result<WaylandObjectKind, WaylandError> {
		self.idmap
			.iter()
			.find(|(k, _)| **k == id)
			.map(|(_, v)| v.0)
			.ok_or_else(|| WaylandError::ObjectNonExistent)
	}

	pub(crate) fn find_obj_by_kind(&self, kind: WaylandObjectKind) -> Result<Wlto, WaylandError> {
		self.idmap
			.iter()
			.find(|(_, v)| v.0 == kind)
			.map(|(_, v)| v.1.clone())
			.ok_or_else(|| WaylandError::ObjectNonExistent)
	}
}

#[derive(Debug)]
pub enum WaylandError {
	ParseError,
	RecvLenBad,
	NotInRegistry,
	IdMapRemovalFail,
	ObjectNonExistent,
	InvalidPixelFormat,
	InvalidOpCode(OpCode, &'static str),
	NoSerial,
	InvalidEnumVariant,
	BufferObjectNotAttached,
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
			WaylandError::ObjectNonExistent => write!(f, "object non existent"),
			WaylandError::InvalidPixelFormat => {
				write!(f, "an invalid pixel format has been received")
			}
			WaylandError::InvalidOpCode(op, int) => {
				write!(f, "an invalid {} opcode has been received on interface {}", op, int)
			}
			WaylandError::NoSerial => write!(f, "no serial has been found"),
			WaylandError::InvalidEnumVariant => {
				write!(f, "an invalid enum variant has been received")
			}
			WaylandError::BufferObjectNotAttached => {
				write!(f, "no buffer rust object had been attached to the surface")
			}
		}
	}
}

impl Error for WaylandError {}

#[macro_export]
macro_rules! drop {
	($t:ty) => {
		impl Drop for $t {
			fn drop(&mut self) {
				println!("dropping {}", stringify!($t));
				if let Err(er) = self.destroy() {
					eprintln!("err ocurred\n{:#?}", er);
				};
			}
		}
	};
}
