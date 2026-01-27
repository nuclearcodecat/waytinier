use std::{cell::RefCell, error::Error, rc::Rc};

use crate::wayland::{
	DebugLevel, EventAction, ExpectRc, God, RcCell, WaylandError, WaylandObject, WaylandObjectKind,
	WeRcGod, WeakCell,
	registry::Registry,
	surface::Surface,
	wire::{FromWirePayload, Id, WireArgument, WireRequest},
};

pub struct XdgWmBase {
	pub id: Id,
	god: WeRcGod,
}

impl XdgWmBase {
	pub fn new_bound(registry: RcCell<Registry>) -> Result<RcCell<Self>, Box<dyn Error>> {
		let mut reg = registry.borrow_mut();
		let obj = Rc::new(RefCell::new(Self {
			id: 0,
			god: reg.god.clone(),
		}));
		let id = reg
			.god
			.upgrade()
			.to_wl_err()?
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::XdgWmBase, obj.clone());
		obj.borrow_mut().id = id;
		reg.bind(id, WaylandObjectKind::XdgWmBase, 1)?;
		Ok(obj)
	}

	pub(crate) fn wl_destroy(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		}
	}

	pub fn destroy(&self) -> Result<(), Box<dyn Error>> {
		self.queue_request(self.wl_destroy())
	}

	pub(crate) fn wl_pong(&self, serial: u32) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 3,
			args: vec![WireArgument::UnInt(serial)],
		}
	}

	pub fn pong(&self, serial: u32) -> Result<(), Box<dyn Error>> {
		self.queue_request(self.wl_pong(serial))
	}

	pub(crate) fn wl_get_xdg_surface(
		&self,
		wl_surface_id: Id,
		xdg_surface_id: Id,
	) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 2,
			args: vec![WireArgument::NewId(xdg_surface_id), WireArgument::Obj(wl_surface_id)],
		}
	}

	pub fn make_xdg_surface(
		&self,
		wl_surface: RcCell<Surface>,
	) -> Result<RcCell<XdgSurface>, Box<dyn Error>> {
		let surf = wl_surface.borrow();
		let surf_id = surf.id;
		let xdgs = Rc::new(RefCell::new(XdgSurface {
			god: surf.god.clone(),
			id: 0,
			is_configured: false,
			wl_surface: Rc::downgrade(&wl_surface),
		}));
		let god = self.god.upgrade().to_wl_err()?;
		let id = god.borrow_mut().wlim.new_id_registered(WaylandObjectKind::XdgSurface, xdgs.clone());
		self.queue_request(self.wl_get_xdg_surface(surf_id, id))?;
		xdgs.borrow_mut().id = id;
		god.borrow_mut().xdg_surface = Some(xdgs.clone());
		Ok(xdgs)
	}
}

pub struct XdgSurface {
	god: WeRcGod,
	pub id: Id,
	pub is_configured: bool,
	pub(crate) wl_surface: WeakCell<Surface>,
}

impl XdgSurface {
	pub(crate) fn wl_get_toplevel(
		&self,
		xdg_toplevel_id: Id,
	) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::NewId(xdg_toplevel_id)],
		}
	}

	pub(crate) fn wl_ack_configure(&self, serial: u32) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 4,
			args: vec![WireArgument::UnInt(serial)],
		}
	}
}

pub struct XdgTopLevel {
	pub id: Id,
	god: WeRcGod,
	_parent: WeakCell<XdgSurface>,
	title: Option<String>,
	appid: Option<String>,
	pub close_requested: bool,
}

impl XdgTopLevel {
	pub fn new_from_xdg_surface(
		xdg_surface: RcCell<XdgSurface>,
		god: RcCell<God>,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let xdgtl = Rc::new(RefCell::new(Self {
			id: 0,
			god: Rc::downgrade(&god),
			_parent: Rc::downgrade(&xdg_surface),
			title: None,
			appid: None,
			close_requested: false,
		}));
		let mut god = god.borrow_mut();
		let id = god.wlim.new_id_registered(WaylandObjectKind::XdgTopLevel, xdgtl.clone());
		{
			let mut tl_borrow = xdgtl.borrow_mut();
			god.wlmm.queue_request(xdg_surface.borrow().wl_get_toplevel(id), tl_borrow.kind());
			tl_borrow.id = id;
		}
		Ok(xdgtl)
	}

	pub(crate) fn wl_set_app_id(&self, id: String) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 3,
			args: vec![WireArgument::String(id)],
		}
	}

	pub fn set_app_id(&mut self, id: &str) -> Result<(), Box<dyn Error>> {
		self.appid = Some(id.to_string());
		self.queue_request(self.wl_set_app_id(id.to_string()))
	}

	pub(crate) fn wl_set_title(&self, id: String) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 2,
			args: vec![WireArgument::String(id)],
		}
	}

	pub fn set_title(&mut self, id: &str) -> Result<(), Box<dyn Error>> {
		self.title = Some(id.to_string());
		self.queue_request(self.wl_set_title(id.to_string()))
	}
}

#[allow(dead_code)]
#[repr(u32)]
#[derive(Debug)]
enum XdgTopLevelStates {
	Maximized = 1,
	Fullscreen,
	Resizing,
	Activated,
	TiledLeft,
	TiledRight,
	TiledTop,
	TiledBottom,
	Suspended,
	ConstrainedLeft,
	ConstrainedRight,
	ConstrainedTop,
	ConstrainedBottom,
}

impl WaylandObject for XdgWmBase {
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
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			// ping
			0 => {
				let serial = u32::from_wire(payload)?;
				pending.push(EventAction::Request(self.wl_pong(serial)));
			}
			inv => return Err(WaylandError::InvalidOpCode(inv, self.kind_as_str()).boxed()),
		}
		Ok(pending)
	}

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::XdgWmBase
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}

impl WaylandObject for XdgSurface {
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
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			// configure
			0 => {
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!("{} | configure received, acking", self.kind_as_str()),
				));
				self.is_configured = true;
				let serial = u32::from_wire(payload)?;
				pending.push(EventAction::Request(self.wl_ack_configure(serial)));
			}
			inv => return Err(WaylandError::InvalidOpCode(inv, self.kind_as_str()).boxed()),
		}
		Ok(pending)
	}

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::XdgSurface
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}

impl WaylandObject for XdgTopLevel {
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
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			// configure
			0 => {
				let w = i32::from_wire(payload)?;
				let h = i32::from_wire(&payload[4..])?;
				let states: Vec<XdgTopLevelStates> = Vec::from_wire(&payload[8..])?
					.iter()
					.map(|en| {
						if (*en as usize) < std::mem::variant_count::<XdgTopLevelStates>() {
							Ok(unsafe { std::mem::transmute::<u32, XdgTopLevelStates>(*en) })
						} else {
							Err(WaylandError::InvalidEnumVariant)
						}
					})
					.collect::<Result<Vec<_>, _>>()?;
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!(
						"{} | configure // w: {}, h: {}, states: {:?}",
						self.kind_as_str(),
						w,
						h,
						states
					),
				));
				if w != 0 && h != 0 {
					pending.push(EventAction::Resize(w, h));
				}
			}
			// close
			1 => {
				self.close_requested = true;
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!("{} | close requested", self.kind_as_str()),
				));
			}
			// configure_bounds
			2 => {
				todo!()
			}
			// wm_capabilities
			3 => {
				todo!()
			}
			inv => return Err(WaylandError::InvalidOpCode(inv, self.kind_as_str()).boxed()),
		}
		Ok(pending)
	}

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::XdgTopLevel
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}
