use std::{
	collections::VecDeque,
	env,
	error::Error,
	fmt::Display,
	io::{IoSlice, IoSliceMut},
	os::{
		fd::{FromRawFd, OwnedFd, RawFd},
		unix::net::{AncillaryData, SocketAncillary, UnixStream},
	},
	path::PathBuf,
};

use crate::{
	CYAN, GREEN, NONE, RED, YELLOW,
	wayland::{DebugLevel, IdentManager, OpCode, WaylandError, WaylandObjectKind},
	wlog,
};

pub type Id = u32;

#[derive(Debug)]
pub struct WireRequest {
	pub sender_id: Id,
	pub opcode: usize,
	pub args: Vec<WireArgument>,
}

#[derive(Debug)]
pub struct WireEventRaw {
	pub recv_id: Id,
	pub opcode: usize,
	pub payload: Vec<u8>,
}

#[derive(Debug)]
pub enum WireArgument {
	Int(i32),
	UnInt(u32),
	// add actual type and helper funs
	FixedPrecision(u32),
	String(String),
	Obj(u32),
	NewId(u32),
	NewIdSpecific(&'static str, u32, u32),
	Arr(Vec<u8>),
	// u32?
	FileDescriptor(RawFd),
}

#[derive(Debug)]
pub enum WireArgumentKind {
	Int,
	UnInt,
	FixedPrecision,
	String,
	Obj,
	NewId,
	NewIdSpecific,
	Arr,
	FileDescriptor,
}

#[derive(Debug)]
pub(crate) enum QueueEntry {
	EventResponse(WireEventRaw),
	Request((WireRequest, WaylandObjectKind)),
	Sync(Id),
}

#[derive(Debug)]
pub(crate) struct MessageManager {
	pub(crate) sock: UnixStream,
	pub(crate) q: VecDeque<QueueEntry>,
}

impl Drop for MessageManager {
	fn drop(&mut self) {
		wlog!(DebugLevel::Important, "wlmm", "destroying self", GREEN, CYAN);
		if let Err(er) = self.discon() {
			wlog!(DebugLevel::Error, "wlmm", format!("failed to discon: {er}"), GREEN, RED);
		} else {
			wlog!(DebugLevel::Error, "wlmm", "discon was successful", GREEN, CYAN);
		}
	}
}

impl Drop for IdentManager {
	fn drop(&mut self) {
		let len = self.idmap.len();
		self.idmap.clear();
		wlog!(
			DebugLevel::Important,
			"wlim",
			format!("destroying self, cleared {len} objects from the map"),
			YELLOW,
			CYAN
		);
	}
}

struct WireDebugMessage<'a> {
	opcode: (Option<String>, OpCode),
	object: (Option<WaylandObjectKind>, Option<Id>),
	args: &'a Vec<WireArgument>,
}

impl Display for WireDebugMessage<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let part1 = if let Some(opcode_str) = &self.opcode.0 {
			format!("{opcode_str} ({}°) ", self.opcode.1)
		} else {
			format!(": opcode {}, ", self.opcode.1)
		};
		let part2 = if let Some(kind) = self.object.0 {
			let mut og = format!("for object {:?}", kind);
			if let Some(id) = self.object.1 {
				og = og + &format!(" ({})", id);
			};
			og
		} else {
			String::from("")
		};
		write!(f, "sending request{}{} with args {:?}", part1, part2, self.args)
	}
}

impl WireRequest {
	fn make_debug(
		&self,
		id: Option<Id>,
		kind: Option<WaylandObjectKind>,
		opcode_name: Option<String>,
	) -> WireDebugMessage<'_> {
		WireDebugMessage {
			opcode: (opcode_name, self.opcode),
			object: (kind, id),
			args: &self.args,
		}
	}
}

impl MessageManager {
	pub fn new(sockname: &str) -> Result<Self, Box<dyn Error>> {
		let base = env::var("XDG_RUNTIME_DIR")?;
		let mut base = PathBuf::from(base);
		base.push(sockname);
		let sock = UnixStream::connect(base)?;
		sock.set_nonblocking(true)?;
		let wlmm = Self {
			sock,
			q: VecDeque::new(),
		};

		Ok(wlmm)
	}

	pub fn from_defualt_env() -> Result<Self, Box<dyn Error>> {
		let env = env::var("WAYLAND_DISPLAY");
		match env {
			Ok(x) => Ok(Self::new(&x)?),
			Err(er) => match er {
				std::env::VarError::NotPresent => Err(WaylandError::NoWaylandDisplay.boxed()),
				_ => Err(Box::new(er)),
			},
		}
	}

	pub fn discon(&self) -> Result<(), Box<dyn Error>> {
		Ok(self.sock.shutdown(std::net::Shutdown::Both)?)
	}

	pub fn send_request_logged(
		&self,
		msg: &mut WireRequest,
		id: Option<Id>,
		kind: Option<WaylandObjectKind>,
		opcode_name: Option<String>,
	) -> Result<(), Box<dyn Error>> {
		let dbugmsg = msg.make_debug(id, kind, opcode_name);
		wlog!(DebugLevel::Trivial, "wlmm", format!("{}", dbugmsg), GREEN, NONE);
		self.send_request(msg)
	}

	pub fn send_request(&self, msg: &mut WireRequest) -> Result<(), Box<dyn Error>> {
		let mut buf: Vec<u8> = vec![];
		buf.append(&mut Vec::from(msg.sender_id.to_ne_bytes()));
		buf.append(&mut vec![0, 0, 0, 0]);
		let mut fds = vec![];
		for obj in msg.args.iter_mut() {
			match obj {
				WireArgument::Arr(x) => {
					let len = x.len() as u32;
					buf.append(&mut Vec::from(len.to_ne_bytes()));
					buf.append(x);
					buf.resize(x.len() - (x.len() % 4) - 4, 0);
				}
				WireArgument::FileDescriptor(x) => {
					fds.push(*x);
				}
				_ => buf.append(&mut obj.as_vec_u8()),
			}
		}
		let word2 = (buf.len() << 16) as u32 | (msg.opcode as u32 & 0x0000ffffu32);
		let word2 = word2.to_ne_bytes();
		for (en, ix) in (4..=7).enumerate() {
			buf[ix] = word2[en];
		}
		let mut ancillary_buf = [0; 128];
		let mut ancillary = SocketAncillary::new(&mut ancillary_buf);
		ancillary.add_fds(&fds);
		wlog!(DebugLevel::SuperVerbose, "wlmm", format!("buf: {buf:?}"), GREEN, NONE);
		self.sock.send_vectored_with_ancillary(&[IoSlice::new(&buf)], &mut ancillary)?;
		Ok(())
	}

	fn get_socket_data(&self, buf: &mut [u8]) -> Result<(usize, Vec<OwnedFd>), Box<dyn Error>> {
		let mut iov = [IoSliceMut::new(buf)];

		let mut aux_buf: [u8; 64] = [0; 64];
		let mut aux = SocketAncillary::new(&mut aux_buf);

		match self.sock.recv_vectored_with_ancillary(&mut iov, &mut aux) {
			Ok(l) => {
				let mut fds = vec![];
				for msg in aux.messages() {
					if let Ok(AncillaryData::ScmRights(scmr)) = msg {
						for fd in scmr {
							let fd = unsafe { OwnedFd::from_raw_fd(fd) };
							fds.push(fd);
						}
					}
				}
				Ok((l, fds))
			}
			Err(er) => match er.kind() {
				std::io::ErrorKind::WouldBlock => Ok((0, vec![])),
				_ => Err(Box::new(er)),
			},
		}
	}

	pub fn get_events(&mut self) -> Result<(usize, Vec<OwnedFd>), Box<dyn Error>> {
		let mut b = [0; 8192];
		let (len, fds) = self.get_socket_data(&mut b)?;
		if len == 0 {
			return Ok((0, vec![]));
		}

		let mut cursor = 0;
		let mut ctr = 0;
		while cursor < len {
			let sender_id =
				u32::from_ne_bytes([b[cursor], b[cursor + 1], b[cursor + 2], b[cursor + 3]]);
			let byte2 =
				u32::from_ne_bytes([b[cursor + 4], b[cursor + 5], b[cursor + 6], b[cursor + 7]]);

			let recv_len = byte2 >> 16;
			// println!("len: {}", recv_len);
			if recv_len < 8 {
				return Err(WaylandError::RecvLenBad.boxed());
			}
			let opcode = (byte2 & 0x0000ffff) as usize;

			let payload = Vec::from(&b[cursor + 8..cursor + recv_len as usize]);

			let event = WireEventRaw {
				recv_id: sender_id,
				opcode,
				payload,
			};
			self.q.push_back(QueueEntry::EventResponse(event));
			ctr += 1;

			cursor += recv_len as usize;
		}
		Ok((ctr, fds))
	}

	pub fn queue_request(&mut self, req: WireRequest, kind: WaylandObjectKind) {
		self.q.push_back(QueueEntry::Request((req, kind)));
	}
}

impl WireArgument {
	// size in bytes
	pub fn size(&self) -> usize {
		match self {
			WireArgument::Int(_) => 4,
			WireArgument::UnInt(_) => 4,
			WireArgument::FixedPrecision(_) => 4,
			WireArgument::String(x) => x.len(),
			WireArgument::Obj(_) => 4,
			WireArgument::NewId(_) => 4,
			WireArgument::NewIdSpecific(x, _, _) => x.len() + 8,
			WireArgument::Arr(x) => x.len(),
			WireArgument::FileDescriptor(_) => 4,
		}
	}

	pub fn as_vec_u8(&self) -> Vec<u8> {
		match self {
			WireArgument::Int(x) => Vec::from(x.to_ne_bytes()),
			WireArgument::UnInt(x) => Vec::from(x.to_ne_bytes()),
			WireArgument::FixedPrecision(x) => Vec::from(x.to_ne_bytes()),
			WireArgument::String(x) => {
				let mut complete: Vec<u8> = vec![];
				// str len + 1 because of nul
				let len = &mut Vec::from(((x.len() + 1) as u32).to_ne_bytes());
				complete.append(len);
				complete.append(&mut Vec::from(x.as_str()));
				// nul
				complete.push(0);
				// padding
				complete.resize(complete.len() - (complete.len() % 4) + 4, 0);
				// println!("complete len rn: {}", complete.len());
				complete
			}
			WireArgument::Obj(x) => Vec::from(x.to_ne_bytes()),
			WireArgument::NewId(x) => Vec::from(x.to_ne_bytes()),
			WireArgument::NewIdSpecific(x, y, z) => {
				let mut complete: Vec<u8> = vec![];
				// str len
				let len = &mut Vec::from(((x.len() + 1) as u32).to_ne_bytes());
				complete.append(len);
				// println!("len: {}, complete: {:?}", complete.len(), complete);
				complete.append(&mut Vec::from(*x));
				// println!("len: {}, complete: {:?}", complete.len(), complete);
				complete.push(0);
				// println!("len: {}, complete: {:?}", complete.len(), complete);
				// pad str
				let clen = complete.len();
				complete.resize(clen - (clen % 4) + (4 * (clen % 4).clamp(0, 1)), 0);
				// println!("len: {}, complete: {:?}", complete.len(), complete);
				complete.append(&mut Vec::from(y.to_ne_bytes()));
				// println!("len: {}, complete: {:?}", complete.len(), complete);
				complete.append(&mut Vec::from(z.to_ne_bytes()));
				// println!("len: {}, complete: {:?}", complete.len(), complete);
				// println!("complete len rn: {}", complete.len());
				complete
			}
			WireArgument::Arr(_) => panic!("debil"),
			WireArgument::FileDescriptor(x) => Vec::from(x.to_ne_bytes()),
		}
	}
}

pub(crate) trait FromWireSingle: Sized {
	const ONE_ELEMENT_SIZE: usize;

	fn from_wire_element(bytes: &[u8]) -> Result<Self, Box<dyn Error>>;
}

impl FromWireSingle for u32 {
	const ONE_ELEMENT_SIZE: usize = 4;

	fn from_wire_element(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
		is_payload_empty(bytes)?;
		Ok(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
	}
}

impl FromWireSingle for i32 {
	const ONE_ELEMENT_SIZE: usize = 4;

	fn from_wire_element(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
		is_payload_empty(bytes)?;
		Ok(i32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
	}
}

impl FromWireSingle for u16 {
	const ONE_ELEMENT_SIZE: usize = 2;

	fn from_wire_element(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
		is_payload_empty(bytes)?;
		Ok(u16::from_ne_bytes([bytes[0], bytes[1]]))
	}
}

impl FromWireSingle for u64 {
	const ONE_ELEMENT_SIZE: usize = 4;

	fn from_wire_element(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
		is_payload_empty(bytes)?;
		Ok(u64::from_ne_bytes([
			bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
		]))
	}
}

pub(crate) trait FromWirePayload: Sized {
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>>;
}

pub(crate) fn is_payload_empty(payload: &[u8]) -> Result<(), Box<dyn Error>> {
	if payload.is_empty() {
		Err(WaylandError::EmptyFromWirePayload.boxed())
	} else {
		Ok(())
	}
}

impl<T> FromWirePayload for Vec<T>
where
	T: FromWireSingle,
{
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>> {
		payload[4..].chunks(T::ONE_ELEMENT_SIZE).map(T::from_wire_element).collect()
	}
}

impl FromWirePayload for String {
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>> {
		is_payload_empty(payload)?;
		let p = payload;
		let len = u32::from_ne_bytes([p[0], p[1], p[2], p[3]]) as usize;
		let ix = p[4..4 + len]
			.iter()
			.enumerate()
			.find(|(_, c)| **c == b'\0')
			.map(|(e, _)| e)
			.unwrap_or_default();
		Ok(String::from_utf8(p[4..4 + ix].to_vec())?)
	}
}

pub(crate) trait FromBits: Sized {
	fn to_enum_vec(bits: u32) -> Vec<Self>;
}
