use std::{cell::RefCell, error::Error, rc::Rc};

use crate::{
	make_drop_impl,
	wayland::{
		DebugLevel, EventAction, God, OpCode, RcCell, WaylandError, WaylandObject,
		WaylandObjectKind, WeRcGod, WeakCell,
		wire::{FromWirePayload, Id, WireArgument, WireRequest},
		xdg_shell::xdg_surface::XdgSurface,
	},
};

pub struct XdgTopLevel {
	pub(crate) id: Id,
	pub(crate) god: WeRcGod,
	pub(crate) parent: WeakCell<XdgSurface>,
	pub(crate) title: Option<String>,
	pub(crate) appid: Option<String>,
	pub(crate) close_requested: bool,
}

impl XdgTopLevel {
	pub fn new_from_xdg_surface(
		xdg_surface: RcCell<XdgSurface>,
		god: RcCell<God>,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let xdgtl = Rc::new(RefCell::new(Self {
			id: 0,
			god: Rc::downgrade(&god),
			parent: Rc::downgrade(&xdg_surface),
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

	pub(crate) fn wl_destroy(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		}
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

impl WaylandObject for XdgTopLevel {
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
					pending.push(EventAction::Resize(w, h, self.parent.clone()));
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

make_drop_impl!(XdgTopLevel, wl_destroy);
