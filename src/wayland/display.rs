use crate::wayland::{
	EventAction, ExpectRc, God, OpCode, RcCell, RecvError, WaylandError, WaylandObject,
	WaylandObjectKind, WeRcGod,
	callback::Callback,
	registry::Registry,
	wire::{FromWirePayload, Id, WireArgument, WireRequest},
};
use std::{cell::RefCell, error::Error, rc::Rc};

pub struct Display {
	pub id: Id,
	god: WeRcGod,
}

impl Display {
	pub fn new(god: RcCell<God>) -> Result<RcCell<Self>, Box<dyn Error>> {
		let display = Rc::new(RefCell::new(Self {
			id: 0,
			god: Rc::downgrade(&god),
		}));
		let id =
			god.borrow_mut().wlim.new_id_registered(WaylandObjectKind::Display, display.clone());
		display.borrow_mut().id = id;
		Ok(display)
	}

	pub fn make_registry(&mut self) -> Result<RcCell<Registry>, Box<dyn Error>> {
		let reg = Rc::new(RefCell::new(Registry::new_empty(0, self.god.clone())));
		let id = self
			.god
			.upgrade()
			.to_wl_err()?
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::Registry, reg.clone());
		reg.borrow_mut().id = id;
		self.wl_get_registry(id)?;
		Ok(reg)
	}

	pub(crate) fn wl_get_registry(&mut self, id: Id) -> Result<(), Box<dyn Error>> {
		self.god.upgrade().to_wl_err()?.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 1,
			args: vec![WireArgument::NewId(id)],
		})
	}

	pub(crate) fn wl_sync(&mut self, id: Id) -> Result<(), Box<dyn Error>> {
		self.god.upgrade().to_wl_err()?.borrow().wlmm.send_request(&mut WireRequest {
			sender_id: self.id,
			opcode: 0,
			args: vec![WireArgument::NewId(id)],
		})
	}

	pub fn sync(&mut self) -> Result<RcCell<Callback>, Box<dyn Error>> {
		let cb = Callback::new(self.god.clone())?;
		let id = self
			.god
			.upgrade()
			.to_wl_err()?
			.borrow_mut()
			.wlim
			.new_id_registered(WaylandObjectKind::Callback, cb.clone());
		self.wl_sync(id)?;
		Ok(cb)
	}
}

impl WaylandObject for Display {
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
				let obj_id = u32::from_wire(p)?;
				let code = u32::from_wire(&p[4..])?;
				let message = String::from_wire(&p[8..])?;
				// maybe add some sort of error manager
				eprintln!("======== ERROR {} FIRED in wl_display\nfor object\n{:?}", code, message);
				pending.push(EventAction::Error(
					RecvError {
						id: obj_id,
						code,
						msg: message,
					}
					.boxed(),
				));
			}
			1 => {
				let deleted_id = u32::from_wire(payload)?;
				// println!(
				// 	"==================== ID {:?} GOT DELETED (unimpl)",
				// 	deleted_id
				// );
				// self.god.upgrade().to_wl_err()?.borrow_mut().wlim.free_id(deleted_id)?;
				pending.push(EventAction::IdDeletion(deleted_id));
			}
			inv => {
				return Err(WaylandError::InvalidOpCode(inv, self.kind_as_str()).boxed());
			}
		}
		Ok(pending)
	}

	#[inline]
	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::Display
	}

	#[inline]
	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}
