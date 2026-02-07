use std::{cell::RefCell, os::fd::OwnedFd, rc::Rc};

use crate::wayland::{
	EventAction, IdentManager, RcCell, WaylandError, WaylandObject, WaylandObjectKind,
	wire::{FromWireSingle, Id},
};

pub struct Callback {
	pub(crate) id: Id,
	pub(crate) done: bool,
	pub(crate) data: Option<u32>,
}

impl Callback {
	pub(crate) fn new() -> RcCell<Self> {
		let cb = Rc::new(RefCell::new(Self {
			id: 0,
			done: false,
			data: None,
		}));
		cb
	}

	pub(crate) fn new_registered(wlim: &mut IdentManager) -> RcCell<Self> {
		let cb = Rc::new(RefCell::new(Self {
			id: 1,
			done: false,
			data: None,
		}));
		let id = wlim.new_id_registered(cb.clone().borrow().kind(), cb.clone());
		cb.borrow_mut().id = id;
		cb
	}
}

impl WaylandObject for Callback {
	fn handle(
		&mut self,
		opcode: super::OpCode,
		payload: &[u8],
		_fds: &[OwnedFd],
	) -> Result<Vec<EventAction>, Box<dyn std::error::Error>> {
		let mut pending = vec![];
		match opcode {
			0 => {
				let data = u32::from_wire_element(payload)?;
				self.done = true;
				self.data = Some(data);
				pending.push(EventAction::CallbackDone(self.id, data));
			}
			inv => {
				return Err(WaylandError::InvalidOpCode(inv, self.kind_as_str()).boxed());
			}
		}
		Ok(pending)
	}

	#[inline]
	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::Callback
	}

	#[inline]
	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}
}
