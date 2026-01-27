use std::{collections::HashMap, error::Error};

use crate::{NONE, WHITE, wayland::{
	DebugLevel, EventAction, OpCode, WaylandError, WaylandObject, WaylandObjectKind,
	WeRcGod,
	wire::{FromWirePayload, Id, WireArgument, WireRequest},
}, wlog};

pub struct Registry {
	pub id: Id,
	pub(crate) inner: HashMap<u32, RegistryEntry>,
	pub(crate) god: WeRcGod,
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct RegistryEntry {
	interface: String,
	version: u32,
}

impl Registry {
	pub fn new_empty(id: Id, god: WeRcGod) -> Self {
		Self {
			id,
			inner: HashMap::new(),
			god,
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
		wlog!(DebugLevel::Important, self.kind_as_str(), format!("bind global id for {}: {}", object.as_str(), global_id), WHITE, NONE);
		self.queue_request(WireRequest {
			// wl_registry id
			sender_id: self.id,
			// first request in the proto
			opcode: 0,
			args: vec![
				WireArgument::UnInt(global_id),
				// WireArgument::NewId(new_id),
				WireArgument::NewIdSpecific(object.as_str(), version, id),
			],
		})
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
				return Err(WaylandError::InvalidOpCode(inv, self.kind_as_str()).boxed());
			}
		}
		Ok(pending)
	}

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::Registry
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}
