use std::{cell::RefCell, error::Error, rc::Rc};

use crate::{
	make_drop_impl,
	wayland::{
		EventAction, ExpectRc, OpCode, RcCell, WaylandError, WaylandObject, WaylandObjectKind,
		WeRcGod,
		registry::Registry,
		surface::Surface,
		wire::{FromWirePayload, Id, WireArgument, WireRequest},
		xdg_shell::xdg_surface::XdgSurface,
	},
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

	pub(crate) fn wl_get_xdg_surface(&self, wl_surface_id: Id, xdg_surface_id: Id) -> WireRequest {
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
		let id =
			god.borrow_mut().wlim.new_id_registered(WaylandObjectKind::XdgSurface, xdgs.clone());
		self.queue_request(self.wl_get_xdg_surface(surf_id, id))?;
		xdgs.borrow_mut().id = id;
		Ok(xdgs)
	}
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
		opcode: OpCode,
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

make_drop_impl!(XdgWmBase, wl_destroy);
