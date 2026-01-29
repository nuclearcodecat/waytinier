use crate::{
	CYAN, DebugLevel, NONE, RED, WHITE, YELLOW, dbug,
	wayland::{
		wire::{Id, MessageManager, QueueEntry, WireRequest},
		xdg_shell::xdg_surface::XdgSurface,
	},
	wlog,
};
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
pub mod registry;
pub mod shm;
pub mod surface;
pub mod wire;
pub mod xdg_shell;

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

pub(crate) enum EventAction {
	Request(WireRequest),
	IdDeletion(Id),
	Error(Box<dyn Error>),
	DebugMessage(DebugLevel, String),
	Resize(i32, i32, WeakCell<XdgSurface>),
	CallbackDone(Id, u32),
}

pub(crate) trait WaylandObject {
	#[allow(dead_code)]
	fn id(&self) -> Id;
	fn god(&self) -> WeRcGod;
	fn handle(
		&mut self,
		opcode: OpCode,
		payload: &[u8],
	) -> Result<Vec<EventAction>, Box<dyn Error>>;
	fn kind_as_str(&self) -> &'static str;
	fn kind(&self) -> WaylandObjectKind;
	fn queue_request(&self, req: WireRequest) -> Result<(), Box<dyn Error>> {
		self.god().upgrade().to_wl_err()?.borrow_mut().wlmm.queue_request(req, self.kind());
		Ok(())
	}
}

pub type WeRcGod = Weak<RefCell<God>>;

pub struct God {
	pub(crate) wlim: IdentManager,
	pub(crate) wlmm: MessageManager,
}

impl God {
	pub(crate) fn new(wlmm: MessageManager, wlim: IdentManager) -> Self {
		Self {
			wlmm,
			wlim,
		}
	}

	pub fn new_default() -> Result<RcCell<Self>, Box<dyn Error>> {
		let wlim = IdentManager::default();
		let wlmm = MessageManager::from_defualt_env()?;
		Ok(Rc::new(RefCell::new(God::new(wlmm, wlim))))
	}

	pub fn handle_events(&mut self) -> Result<(), Box<dyn Error>> {
		wlog!(DebugLevel::Trivial, "event handler", "called", CYAN, NONE);
		let mut retries = 0;
		while self.wlmm.get_events()? == 0 && retries < 9999 {
			retries += 1;
		}
		let mut last_id: Id = 0;
		let mut actions: VecDeque<(EventAction, WaylandObjectKind, Id)> = VecDeque::new();
		while let Some(entry) = self.wlmm.q.pop_front() {
			match entry {
				QueueEntry::EventResponse(ev) => {
					let obj = match self.wlim.find_obj_by_id(ev.recv_id) {
						Ok(o) => o,
						Err(er) => match er {
							WaylandError::ObjectNonExistent => {
								wlog!(
									DebugLevel::Important,
									"event handler",
									"received an event for a nonexistent object",
									CYAN,
									YELLOW
								);
								continue;
							}
							_ => return Err(er.boxed()),
						},
					};
					let resulting_actions = obj.1.borrow_mut().handle(ev.opcode, &ev.payload)?;
					let x: Vec<(EventAction, WaylandObjectKind, Id)> =
						resulting_actions.into_iter().map(|x| (x, obj.0, ev.recv_id)).collect();
					actions.extend(x);
				}
				QueueEntry::Request((req, kind)) => {
					let id = req.sender_id;
					actions.push_back((EventAction::Request(req), kind, id));
				}
				QueueEntry::Sync(id) => {
					self.wlim.current_sync_id = Some(id);
				}
			}
		}
		while let Some((act, kind, id)) = actions.pop_front() {
			if last_id != id {
				wlog!(
					DebugLevel::Trivial,
					"event handler",
					format!("going to handle {:?} ({id})", kind),
					CYAN,
					NONE
				);
				last_id = id;
			}
			match act {
				EventAction::Request(mut msg) => {
					self.wlmm.send_request_logged(&mut msg, Some(id), Some(kind), None)?;
				}
				EventAction::IdDeletion(id) => {
					wlog!(
						DebugLevel::Trivial,
						"event handler",
						format!("id {id} deleted internally"),
						CYAN,
						NONE
					);
					self.wlim.free_id(id)?;
				}
				EventAction::Error(er) => wlog!(DebugLevel::Error, "event handler", er, RED, RED),
				EventAction::DebugMessage(lvl, msg) => {
					let tcol = if lvl == DebugLevel::Error {
						RED
					} else {
						WHITE
					};
					wlog!(lvl, kind.as_str(), msg, WHITE, tcol);
				}
				EventAction::Resize(w, h, xdgs) => {
					let xdgs = xdgs.upgrade().to_wl_err()?;
					let xdgs = xdgs.borrow_mut();
					let surf = xdgs.wl_surface.upgrade().to_wl_err()?;
					let mut surf = surf.borrow_mut();

					if let Some(buf_) = surf.attached_buf.clone() {
						let mut buf = buf_.borrow_mut();
						wlog!(
							DebugLevel::Important,
							"event handler",
							format!("calling resize, w: {}, h: {}", w, h),
							CYAN,
							NONE
						);
						let new_buf_id =
							self.wlim.new_id_registered(WaylandObjectKind::Buffer, buf_.clone());
						let acts = buf.resize(new_buf_id, (w, h))?;
						actions.extend_front(acts);
					} else {
						wlog!(
							DebugLevel::Important,
							"event handler",
							"buf not present at resize",
							CYAN,
							YELLOW
						);
					}

					surf.w = w;
					surf.h = h;
				}
				EventAction::CallbackDone(id, data) => {
					wlog!(
						DebugLevel::Trivial,
						"event handler",
						format!("callback {} done with data {}", id, data),
						CYAN,
						NONE
					);
					if let Some(sid) = self.wlim.current_sync_id
						&& sid == id
					{
						self.wlim.current_sync_id = None;
						break;
					}
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
	pub(crate) fn as_str(&self) -> &'static str {
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
pub(crate) struct IdentManager {
	pub(crate) top_id: Id,
	pub(crate) free: VecDeque<Id>,
	pub(crate) idmap: HashMap<Id, (WaylandObjectKind, Wlto)>,
	pub(crate) current_sync_id: Option<Id>,
}

impl IdentManager {
	pub(crate) fn new_id(&mut self) -> Id {
		self.top_id += 1;
		wlog!(DebugLevel::Trivial, "wlim", format!("new id picked: {}", self.top_id), YELLOW, NONE);
		self.top_id
	}

	pub(crate) fn new_id_registered(&mut self, kind: WaylandObjectKind, obj: Wlto) -> Id {
		wlog!(
			DebugLevel::Trivial,
			"wlim",
			format!("picking new id for {}", kind.as_str()),
			YELLOW,
			NONE
		);
		let id = if let Some(id) = self.free.pop_front() {
			wlog!(
				DebugLevel::Trivial,
				"wlim",
				format!("new id picked from free pool: {}", self.top_id),
				YELLOW,
				NONE
			);
			id
		} else {
			self.new_id()
		};
		self.idmap.insert(id, (kind, obj));
		id
	}

	pub(crate) fn free_id(&mut self, id: Id) -> Result<(), Box<dyn Error>> {
		let registered = self.idmap.iter().find(|(k, _)| **k == id).map(|(k, _)| k).copied();
		if let Some(r) = registered {
			self.idmap.remove(&r).ok_or(WaylandError::IdMapRemovalFail.boxed())?;
		}
		self.free.push_back(id);
		wlog!(
			DebugLevel::Trivial,
			"wlim",
			format!("freeing id {id} | all: {:?}", self.free),
			YELLOW,
			NONE
		);
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
	NoWaylandDisplay,
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
			WaylandError::NoWaylandDisplay => {
				write!(f, "WAYLAND_DISPLAY is not set")
			}
		}
	}
}

impl Error for WaylandError {}

// return crate later when finished with the spawner
pub trait ExpectRc<T> {
	fn to_wl_err(self) -> Result<Rc<T>, Box<dyn Error>>;
}

impl<T> ExpectRc<T> for Option<Rc<T>> {
	fn to_wl_err(self) -> Result<Rc<T>, Box<dyn Error>> {
		match self {
			Some(x) => Ok(x),
			None => Err(WaylandError::ObjectNonExistentInWeak.boxed()),
		}
	}
}

impl Drop for God {
	fn drop(&mut self) {
		wlog!(DebugLevel::Important, "god", "dropping self", RED, CYAN);
	}
}
