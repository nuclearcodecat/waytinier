use std::{collections::HashMap, error::Error};

use crate::wayland::{
	CtxType, DebugLevel, EventAction, ExpectRc, OpCode, WaylandError, WaylandObject,
	WaylandObjectKind,
	wire::{FromWirePayload, Id, WireArgument, WireRequest},
};

pub struct Registry {
	pub id: Id,
	pub(crate) inner: HashMap<u32, RegistryEntry>,
	pub(crate) ctx: CtxType,
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct RegistryEntry {
	interface: String,
	version: u32,
}

impl Registry {
	pub fn new_empty(id: Id, ctx: CtxType) -> Self {
		Self {
			id,
			inner: HashMap::new(),
			ctx,
		}
	}

	fn wl_bind(
		&mut self,
		id: Id,
		object: WaylandObjectKind,
		version: u32,
	) -> Result<(), Box<dyn Error>> {
		let global_id = self
			.inner
			.iter()
			.find(|(_, v)| v.interface == object.as_str())
			.map(|(k, _)| k)
			.copied()
			.ok_or(WaylandError::NotInRegistry)?;
		println!("bind global id for {}: {}", object.as_str(), global_id);

		self.ctx.upgrade().to_wl_err()?.borrow().wlmm.send_request(&mut WireRequest {
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

	pub(crate) fn bind(
		&mut self,
		id: Id,
		object: WaylandObjectKind,
		version: u32,
	) -> Result<(), Box<dyn Error>> {
		self.wl_bind(id, object, version)?;
		Ok(())
	}

	pub fn does_implement(&self, query: &str) -> Option<u32> {
		self.inner.iter().find(|(_, v)| v.interface == query).map(|(_, v)| v.version)
	}
}

impl WaylandObject for Registry {
	fn handle(
		&mut self,
		opcode: OpCode,
		payload: &[u8],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let p = payload;
		let mut pending = vec![];
		match opcode {
			0 => {
				let name = u32::from_wire(p)?;
				let interface = String::from_wire(&p[4..])?;
				let version = u32::from_wire(&p[p.len() - 4..])?;
				let msg = format!("inserted interface {} version {}", interface, version);
				self.inner.insert(
					name,
					RegistryEntry {
						interface,
						version,
					},
				);
				pending.push(EventAction::DebugMessage(DebugLevel::Trivial, msg));
			}
			// can global_remove even happen
			1 => {
				// let name = decode_event_payload(&p[8..], WireArgumentKind::UnInt)?;
				todo!()
			}
			inv => {
				return Err(WaylandError::InvalidOpCode(inv, self.as_str()).boxed());
			}
		}
		Ok(pending)
	}

	fn as_str(&self) -> &'static str {
		WaylandObjectKind::Registry.as_str()
	}
}
