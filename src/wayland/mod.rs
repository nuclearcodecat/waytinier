use crate::{CYAN, NONE, RED, YELLOW, wayland::{
	wire::{Id, MessageManager, WireRequest},
	xdgshell::XdgSurface,
}, wlog};
use std::{
	cell::RefCell,
	collections::{HashMap, VecDeque},
	error::Error,
	fmt::{self, Display},
	rc::{Rc, Weak},
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

#[allow(dead_code)]
#[repr(isize)]
pub(crate) enum DebugLevel {
	None,
	Error,
	Important,
	Trivial,
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

pub type CtxType = Weak<RefCell<Context>>;

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

	pub fn new_default() -> Result<RcCell<Self>, Box<dyn Error>> {
		let wlim = IdentManager::default();
		let wlmm = MessageManager::from_defualt_env()?;
		Ok(Rc::new(RefCell::new(Context::new(wlmm, wlim))))
	}

	pub fn handle_events(&mut self) -> Result<(), Box<dyn Error>> {
		let mut retries = 0;
		while self.wlmm.get_events()? == 0 && retries < 9999 {
			retries += 1;
		}
		let mut actions: VecDeque<EventAction> = VecDeque::new();
		while let Some(ev) = self.wlmm.q.pop_front() {
			let obj = self.wlim.find_obj_by_id(ev.recv_id)?;
			wlog!(DebugLevel::Trivial, "event handler", format!("going to handle {:?}", obj.0), CYAN, NONE);
			let resulting_actions = obj.1.borrow_mut().handle(ev.opcode, &ev.payload)?;
			actions.extend(resulting_actions);
		}
		while let Some(act) = actions.pop_front() {
			match act {
				EventAction::Request(mut msg) => {
					self.wlmm.send_request(&mut msg)?;
				}
				EventAction::IdDeletion(id) => {
					wlog!(DebugLevel::Trivial, "event handler", format!("id {id} deleted internally"), CYAN, NONE);
					self.wlim.free_id(id)?;
				}
				// add colors
				// EventAction::Error(er) => eprintln!("{RED}{:?}{NONE}", er),
				EventAction::Error(er) => wlog!(DebugLevel::Error, "event handler", er, RED, RED),
				// add colors
				EventAction::DebugMessage(lvl, msg) => {
					wlog!(lvl, "wlto", msg, YELLOW, NONE);
				}
				EventAction::Resize(w, h) => {
					let xdgs = self.xdg_surface.clone().ok_or(WaylandError::ObjectNonExistent)?;
					let xdgs = xdgs.borrow_mut();
					let surf = xdgs.wl_surface.upgrade().to_wl_err()?;
					let surf = surf.borrow();
					let buf_ = surf.attached_buf.clone().ok_or(WaylandError::ObjectNonExistent)?;
					let mut buf = buf_.borrow_mut();
					wlog!(DebugLevel::Important, "event handler", format!("calling resize, w: {}, h: {}", w, h), CYAN, NONE);
					let new_buf_id =
						self.wlim.new_id_registered(WaylandObjectKind::Buffer, buf_.clone());
					let acts = buf.resize(new_buf_id, (w, h))?;
					actions.extend(acts);
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
pub type WeakCell<T> = Weak<RefCell<T>>;

#[derive(Default)]
pub struct IdentManager {
	top_id: Id,
	free: VecDeque<Id>,
	idmap: HashMap<Id, (WaylandObjectKind, Wlto)>,
}

impl IdentManager {
	pub(crate) fn new_id(&mut self) -> Id {
		self.top_id += 1;
		wlog!(DebugLevel::Trivial, "idman", format!("new id picked: {}", self.top_id), YELLOW, NONE);
		self.top_id
	}

	pub(crate) fn new_id_registered(&mut self, kind: WaylandObjectKind, obj: Wlto) -> Id {
		let id = self.free.pop_front().unwrap_or_else(|| self.new_id());
		self.idmap.insert(id, (kind, obj));
		id
	}

	pub(crate) fn free_id(&mut self, id: Id) -> Result<(), Box<dyn Error>> {
		let registered = self.idmap.iter().find(|(k, _)| **k == id).map(|(k, _)| k).copied();
		if let Some(r) = registered {
			self.idmap.remove(&r).ok_or(WaylandError::IdMapRemovalFail.boxed())?;
		}
		self.free.push_back(id);
		wlog!(DebugLevel::Trivial, "idman", format!("freeing id {id} | all: {:?}", self.free), YELLOW, NONE);
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
			.ok_or(WaylandError::ObjectNonExistent)
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
	ObjectNonExistentInWeak,
	RequiredValueNone,
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
			WaylandError::ObjectNonExistentInWeak => {
				write!(f, "no object was found inside a weak reference")
			}
			WaylandError::RequiredValueNone => {
				write!(f, "an option with a required value was None")
			}
		}
	}
}

impl Error for WaylandError {}

pub(crate) trait ExpectRc<T> {
	fn to_wl_err(self) -> Result<RcCell<T>, Box<dyn Error>>;
}
impl<T> ExpectRc<T> for Option<Rc<RefCell<T>>> {
	fn to_wl_err(self) -> Result<RcCell<T>, Box<dyn Error>> {
		match self {
			Some(x) => Ok(x),
			None => Err(WaylandError::ObjectNonExistentInWeak.boxed()),
		}
	}
}
