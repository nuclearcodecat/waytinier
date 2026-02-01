// kernel info (see userspace iface notes section and also has dmabuf-specific ioctl info)
// https://docs.kernel.org/driver-api/dma-buf.html
//
// pixel format stuff
// https://docs.kernel.org/userspace-api/dma-buf-alloc-exchange.html
//
// fourcc codes for the modifiers and formats
// https://github.com/torvalds/linux/blob/master/include/uapi/drm/drm_fourcc.h
//
// FINALLY FOUND RENDER NODE INFO (MENTIONED IN WL DOCS), READ THIS LATER
// https://www.kernel.org/doc/html/v4.8/gpu/drm-uapi.html

use std::{
	cell::RefCell,
	error::Error,
	os::fd::{AsRawFd, OwnedFd},
	ptr::null_mut,
	rc::Rc,
};

use libc::{MAP_FAILED, MAP_PRIVATE, PROT_READ};

use crate::{
	DebugLevel, NONE, WHITE,
	abstraction::dma::DRM_FORMAT_ARGB8888,
	dbug,
	wayland::{
		EventAction, ExpectRc, God, OpCode, RcCell, WaylandError, WaylandObject, WaylandObjectKind,
		WeRcGod, WeakCell,
		registry::Registry,
		wire::{FromWirePayload, FromWireSingle, Id, WireArgument, WireRequest},
	},
	wlog,
};

pub(crate) struct DmaBuf {
	pub(crate) id: Id,
	pub(crate) god: WeakCell<God>,
}

impl DmaBuf {
	pub(crate) fn new(god: RcCell<God>) -> Self {
		Self {
			id: 0,
			god: Rc::downgrade(&god),
		}
	}

	pub(crate) fn new_bound(
		registry: RcCell<Registry>,
		god: RcCell<God>,
	) -> Result<RcCell<Self>, Box<dyn Error>> {
		let me = Rc::new(RefCell::new(Self::new(god.clone())));
		let id = god.borrow_mut().wlim.new_id_registered(WaylandObjectKind::DmaBuf, me.clone());
		me.borrow_mut().id = id;
		registry.borrow_mut().bind(id, me.borrow().kind(), 5)?;
		Ok(me)
	}

	pub(crate) fn wl_get_default_feedback(&self, id: Id) -> WireRequest {
		WireRequest {
			sender_id: self.id,
			opcode: 2,
			args: vec![WireArgument::NewId(id)],
		}
	}

	pub(crate) fn get_default_feedback(&mut self) -> Result<RcCell<DmaFeedback>, Box<dyn Error>> {
		let fb = Rc::new(RefCell::new(DmaFeedback::new()));
		let id = self
			.god
			.upgrade()
			.to_wl_err()?
			.borrow_mut()
			.wlim
			.new_id_registered(fb.borrow().kind(), fb.clone());
		fb.borrow_mut().id = id;
		dbug!(format!("{}", id));
		self.queue_request(self.wl_get_default_feedback(id))?;
		Ok(fb)
	}
}

impl WaylandObject for DmaBuf {
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
		_fds: &[OwnedFd],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			// format
			0 => {
				dbug!(format!("format: {:?}", payload));
				pending.push(EventAction::DebugMessage(
					crate::DebugLevel::Important,
					format!("format for dmabuf: {:?}", payload),
				));
			}
			inv => {
				return Err(WaylandError::InvalidOpCode(inv as OpCode, self.kind_as_str()).boxed());
			}
		};
		Ok(pending)
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::DmaBuf
	}
}

#[allow(dead_code)]
pub(crate) struct DmaFeedback {
	pub(crate) id: Id,
	pub(crate) done: bool,
	pub(crate) format_table: Vec<(u32, u64)>,
	pub(crate) format_indices: Vec<u16>,
	pub(crate) flags: Vec<TrancheFlags>,
}

impl DmaFeedback {
	pub(crate) fn new() -> Self {
		Self {
			id: 0,
			done: false,
			format_table: vec![],
			format_indices: vec![],
			flags: vec![],
		}
	}

	fn parse_format_table(&mut self, slice: &[u8]) -> Result<(), Box<dyn Error>> {
		for chunk in slice.chunks(16) {
			let format = u32::from_wire_element(chunk)?;
			let _padding = u32::from_wire_element(&chunk[4..])?;
			let modifier = u64::from_wire_element(&chunk[8..])?;
			self.format_table.push((format, modifier));
		}
		wlog!(
			DebugLevel::Important,
			self.kind_as_str(),
			format!("parsed {} format table: {:?}", self.kind_as_str(), self.format_table),
			WHITE,
			NONE
		);
		Ok(())
	}
}

#[repr(u32)]
#[derive(Debug)]
pub(crate) enum TrancheFlags {
	Scanout = 1 << 0,
}

impl WaylandObject for DmaFeedback {
	fn id(&self) -> Id {
		self.id
	}

	fn god(&self) -> WeRcGod {
		panic!("god is dead")
	}

	fn handle(
		&mut self,
		opcode: OpCode,
		payload: &[u8],
		_fds: &[OwnedFd],
	) -> Result<Vec<EventAction>, Box<dyn Error>> {
		let mut pending = vec![];
		match opcode {
			// done
			0 => {
				self.done = true;
			}
			// format_table
			1 => {
				dbug!("format_table");
				let size = u32::from_wire_element(payload)? as usize;
				let fd = _fds.first().ok_or(WaylandError::FdExpected.boxed())?;
				let ptr = unsafe {
					libc::mmap(null_mut(), size, PROT_READ, MAP_PRIVATE, fd.as_raw_fd(), 0)
				};
				if ptr == MAP_FAILED {
					return Err(Box::new(std::io::Error::last_os_error()));
				}
				let slice: &[u8] = unsafe { std::slice::from_raw_parts(ptr as *mut u8, size) };
				self.parse_format_table(slice)?;

				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!("size: {size}, fd: {:?}", _fds),
				));
			}
			// main_device
			2 => {
				dbug!("main_device");
				let main_device: Vec<u32> = Vec::from_wire(payload)?;
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!("main_device: {:?}", main_device),
				));
			}
			// tranche_done
			3 => {
				dbug!("tranche_done");
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					String::from("tranche done"),
				));
			}
			// tranche_target_device
			4 => {
				dbug!("tranche_target_device");
				let target_device: Vec<u32> = Vec::from_wire(payload)?;
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!("tranche target device: {:?}", target_device),
				));
			}
			// tranche_formats
			5 => {
				dbug!("tranche_formats");
				let indices: Vec<u16> = Vec::from_wire(payload)?;
				self.format_indices = indices;
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!("tranche indices: {:?}", self.format_indices),
				));
				for ix in &self.format_indices {
					let entry = self.format_table[*ix as usize];
					if entry.0 == DRM_FORMAT_ARGB8888 {
						dbug!(format!("found argb8888: {:?}", entry));
					}
					pending.push(EventAction::DebugMessage(
						DebugLevel::Important,
						format!("tranche format {ix}: {:?}", entry),
					));
				}
			}
			// tranche_flags
			6 => {
				dbug!("tranche_flags");
				let flags = u32::from_wire_element(payload)?;
				let mut v = vec![];
				if flags & TrancheFlags::Scanout as u32 != 0 {
					v.push(TrancheFlags::Scanout);
				};
				pending.push(EventAction::DebugMessage(
					DebugLevel::Important,
					format!("tranche flags: {:?}", v),
				));
			}
			inv => {
				return Err(WaylandError::InvalidOpCode(inv as OpCode, self.kind_as_str()).boxed());
			}
		}
		Ok(pending)
	}

	fn kind_as_str(&self) -> &'static str {
		self.kind().as_str()
	}

	fn kind(&self) -> WaylandObjectKind {
		WaylandObjectKind::DmaFeedback
	}
}
