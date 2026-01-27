use std::{
	collections::VecDeque,
	env,
	error::Error,
	io::{IoSlice, Read},
	os::{
		fd::RawFd,
		unix::net::{SocketAncillary, UnixStream},
	},
	path::PathBuf,
};

use crate::{
	GREEN, NONE,
	wayland::{DebugLevel, WaylandError},
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
pub struct MessageManager {
	pub sock: UnixStream,
	pub q: VecDeque<WireEventRaw>,
}

impl Drop for MessageManager {
	fn drop(&mut self) {
		println!("called drop for MessageManager");
		let r = self.discon();
		if r.is_err() {
			eprintln!("failed to drop MessageManager\n{:#?}", r);
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

	pub fn send_request(&self, msg: &mut WireRequest) -> Result<(), Box<dyn Error>> {
		wlog!(DebugLevel::Trivial, "wlmm", format!("sending request {:?}", msg), GREEN, NONE);
		// println!("==== SEND_REQUEST CALLED");
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
					fds.push(*x as RawFd);
				}
				_ => buf.append(&mut obj.as_vec_u8()),
			}
		}
		let word2 = (buf.len() << 16) as u32 | (msg.opcode as u32 & 0x0000ffffu32);
		// println!("=== WORD2\n0b{:0b}\nlen: {}\nopcode: {}", word2, word2 >> 16, word2 & 0x0000ffff);
		let word2 = word2.to_ne_bytes();
		for (en, ix) in (4..=7).enumerate() {
			buf[ix] = word2[en];
		}
		let mut ancillary_buf = [0; 128];
		let mut ancillary = SocketAncillary::new(&mut ancillary_buf);
		ancillary.add_fds(&fds);
		self.sock.send_vectored_with_ancillary(&[IoSlice::new(&buf)], &mut ancillary)?;
		// self.sock.write_all(&buf)?;
		// println!(
		// 	// "=== REQUEST SENT\n{:#?}\n{:?}\nbuf len: {}\naux: {:?}\n\n",
		// 	"=== REQUEST SENT\n{:#?}\n{:?}\nbuf len: {}\n\n",
		// 	msg,
		// 	buf,
		// 	buf.len(),
		// 	// ancillary
		// );
		Ok(())
	}

	fn get_socket_data(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Box<dyn Error>> {
		let len;
		match self.sock.read(buf) {
			Ok(l) => {
				len = l;
			}
			Err(er) => match er.kind() {
				std::io::ErrorKind::WouldBlock => return Ok(None),
				_ => {
					return Err(Box::new(er));
				}
			},
		}
		Ok(Some(len))
	}

	pub fn get_events(&mut self) -> Result<usize, Box<dyn Error>> {
		let mut b = [0; 8192];
		let len = self.get_socket_data(&mut b)?;
		if len.is_none() {
			return Ok(0);
		}
		let len = len.unwrap();

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
			self.q.push_back(event);
			ctr += 1;

			cursor += recv_len as usize;
		}
		Ok(ctr)
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

pub trait FromWirePayload: Sized {
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>>;
}

impl FromWirePayload for String {
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>> {
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

impl FromWirePayload for u32 {
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>> {
		let p = payload;
		Ok(u32::from_ne_bytes([p[0], p[1], p[2], p[3]]))
	}
}

impl FromWirePayload for i32 {
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>> {
		let p = payload;
		Ok(i32::from_ne_bytes([p[0], p[1], p[2], p[3]]))
	}
}

// impl FromWirePayload for Vec<u8> {
// 	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>> {
// 		let size = u32::from_wire(payload)? as usize;
// 		let mut vec = vec![0; size];
// 		vec.extend_from_slice(&payload[4 .. 4 + size]);
// 		Ok(vec)
// 	}
// }

impl FromWirePayload for Vec<u32> {
	fn from_wire(payload: &[u8]) -> Result<Self, Box<dyn Error>> {
		// let size = u32::from_wire(payload)? as usize;
		payload[4..].chunks(4).map(|chunk| u32::from_wire(chunk)).collect()
	}
}
