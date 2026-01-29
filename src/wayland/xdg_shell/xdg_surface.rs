use std::error::Error;

use crate::{
	make_drop_impl,
	wayland::{
		DebugLevel, EventAction, OpCode, WaylandError, WaylandObject, WaylandObjectKind, WeRcGod,
		WeakCell,
		surface::Surface,
		wire::{FromWirePayload, Id, WireArgument, WireRequest},
	},
};

pub struct XdgSurface {
	pub(crate) god: WeRcGod,
	pub id: Id,
	pub is_configured: bool,
	pub(crate) wl_surface: WeakCell<Surface>,
}

impl XdgSurface {
	pub(crate) fn wl_get_toplevel(&self, xdg_toplevel_id: Id) -> WireRequest {
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

	pub(crate) fn wl_destroy(&self) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![],
		}
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
		opcode: OpCode,
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

make_drop_impl!(XdgSurface, wl_destroy);
