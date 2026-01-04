use std::{
	env,
	error::Error,
	io::{Read, Write},
	os::unix::net::UnixStream,
	path::PathBuf,
};

use crate::wayland::{WaylandError, WaylandObjectKind};

#[derive(Debug)]
pub struct WireMessage {
	pub sender_id: u32,
	pub opcode: usize,
	pub args: Vec<WireArgument>,
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
	FileDescriptor(i32),
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

pub struct MessageManager {
	pub sock: UnixStream,
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
		let wlmm = Self { sock };

		Ok(wlmm)
	}

	pub fn discon(&self) -> Result<(), Box<dyn Error>> {
		Ok(self.sock.shutdown(std::net::Shutdown::Both)?)
	}

	pub fn send_request(&mut self, msg: &mut WireMessage) -> Result<(), Box<dyn Error>> {
		println!("==== SEND_REQUEST CALLED");
		let mut buf: Vec<u8> = vec![];
		buf.append(&mut Vec::from(msg.sender_id.to_ne_bytes()));
		let len = {
			// header is 8
			let mut complete = 8;
			for n in msg.args.iter() {
				let size = n.size();
				complete += size;
			}
			complete
		};
		let word2 = (len << 16) as u32 | (msg.opcode as u32 & 0x0000ffffu32);
		println!("=== WORD2\n0b{:0b}\nlen: {}\nopcode: {}", word2, word2 >> 16, word2 & 0x0000ffff);
		buf.append(&mut Vec::from(word2.to_ne_bytes()));
		for obj in msg.args.iter_mut() {
			match obj {
				WireArgument::Arr(x) => {
					buf.append(x);
					while x.len() % 4 > 0 {
						buf.push(0);
					}
				}
				_ => buf.append(&mut obj.as_vec_u8()),
			}
		}
		self.sock.write_all(&buf)?;
		println!("=== REQUEST SENT\n{:#?}\n{:?}\n\n", msg, buf);
		Ok(())
	}

	pub fn get_events_blocking(
		&mut self,
		id: u32,
		kind: WaylandObjectKind,
	) -> Result<Vec<WireMessage>, Box<dyn Error>> {
		let mut read = self.get_events(id, &kind)?;
		let mut retries = 0;
		while read.is_none() && retries <= 30 {
			read = self.get_events(id, &kind)?;
			retries += 1;
		}
		Ok(read.unwrap())
	}

	fn get_socket_data(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Box<dyn Error>> {
		let len;
		match self.sock.read(buf) {
			Ok(l) => {
				len = l;
			}
			Err(er) => {
				match er.kind() {
					std::io::ErrorKind::WouldBlock => return Ok(None),
					_ => {
						return Err(Box::new(er));
					}
				}
			}
		}
		Ok(Some(len))
	}

	pub fn get_events(
		&mut self,
		obj_id: u32,
		kind: &WaylandObjectKind,
	) -> Result<Option<Vec<WireMessage>>, Box<dyn Error>> {
		let mut b = [0; 8192];
		let len = self.get_socket_data(&mut b)?;
		if len.is_none() {
			return Ok(None);
		}
		let len = len.unwrap();

		let mut events = vec![];
		let mut cursor = 0;
		let mut cursor_last = 0;
		while cursor < len {
			let sender_id =
				u32::from_ne_bytes([b[cursor], b[cursor + 1], b[cursor + 2], b[cursor + 3]]);
			let byte2 =
				u32::from_ne_bytes([b[cursor + 4], b[cursor + 5], b[cursor + 6], b[cursor + 7]]);

			let recv_len = byte2 >> 16;
			// println!("len: {}", recv_len);
			if recv_len < 8 {
				eprintln!("recv_len bad");
				return Err(WaylandError::RecvLenBad.boxed());
			}
			let opcode = (byte2 & 0x0000ffff) as usize;

			let mut args = vec![];

			// if err occured
			if sender_id == 1 && opcode == 0 {
				let obj_id = decode_event_payload(&b[cursor + 8..], WireArgumentKind::Obj)?;
				let code = decode_event_payload(&b[cursor + 12..], WireArgumentKind::UnInt)?;
				let message = decode_event_payload(&b[cursor + 16..], WireArgumentKind::String)?;
				eprintln!(
					"======== ERROR FIRED in wl_display\n{:?}",
					message
				);
				args.push(obj_id);
				args.push(code);
				args.push(message);
			}
			if sender_id == obj_id {
				match kind {
					WaylandObjectKind::Display => match opcode {
						1 => {
							let deleted_id =
								decode_event_payload(&b[cursor + 8..], WireArgumentKind::UnInt)?;
							args.push(deleted_id);
						}
						_ => {
							eprintln!("unimplemented display event");
						}
					},
					WaylandObjectKind::Registry => match opcode {
						0 => {
							let name =
								decode_event_payload(&b[cursor + 8..], WireArgumentKind::UnInt)?;
							let interface =
								decode_event_payload(&b[cursor + 12..], WireArgumentKind::String)?;
							let version =
								decode_event_payload(&b[..len - 4], WireArgumentKind::UnInt)?;
							args.push(name);
							args.push(interface);
							args.push(version);
						}
						1 => {
							let name =
								decode_event_payload(&b[cursor + 8..], WireArgumentKind::UnInt)?;
							args.push(name);
						}
						_ => {
							eprintln!("unimplemented registry event");
						}
					},
					_ => eprintln!("unimplemented interface"),
				}
			}

			let event = WireMessage {
				sender_id,
				opcode,
				args,
			};
			events.push(event);

			cursor = cursor_last + recv_len as usize;
			cursor_last = cursor;
		}
		Ok(Some(events))
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
				// str len
				complete.append(&mut Vec::from(x.len().to_ne_bytes()));
				complete.append(&mut Vec::from(x.as_str()));
				// pad str
				complete.resize(complete.len() + complete.len() % 4, 0);
				complete
			}
			WireArgument::Obj(x) => Vec::from(x.to_ne_bytes()),
			WireArgument::NewId(x) => Vec::from(x.to_ne_bytes()),
			WireArgument::NewIdSpecific(x, y, z) => {
				let mut complete: Vec<u8> = vec![];
				// str len
				complete.append(&mut Vec::from(x.len().to_ne_bytes()));
				complete.append(&mut Vec::from(*x));
				// pad str
				complete.resize(complete.len() + complete.len() % 4, 0);
				complete.append(&mut Vec::from(y.to_ne_bytes()));
				complete.append(&mut Vec::from(z.to_ne_bytes()));
				complete
			}
			WireArgument::Arr(_) => panic!("debil"),
			WireArgument::FileDescriptor(x) => Vec::from(x.to_ne_bytes()),
		}
	}
}

fn decode_event_payload(
	payload: &[u8],
	kind: WireArgumentKind,
) -> Result<WireArgument, Box<dyn Error>> {
	let p = payload;
	match kind {
		WireArgumentKind::Int
		| WireArgumentKind::Obj
		| WireArgumentKind::NewId
		| WireArgumentKind::FileDescriptor
		| WireArgumentKind::FixedPrecision => Ok(WireArgument::Int(i32::from_ne_bytes([
			p[0], p[1], p[2], p[3],
		]))),
		WireArgumentKind::UnInt => Ok(WireArgument::UnInt(u32::from_ne_bytes([
			p[0], p[1], p[2], p[3],
		]))),
		WireArgumentKind::String => {
			let len = u32::from_ne_bytes([p[0], p[1], p[2], p[3]]) as usize;
			let ix = p[4..4 + len]
				.iter()
				.enumerate()
				.find(|(_, c)| **c == b'\0')
				.map(|(e, _)| e)
				.unwrap_or_default();
			Ok(WireArgument::String(String::from_utf8(
				p[4..4 + ix].to_vec(),
			)?))
		}
		// not sure how to handle this
		WireArgumentKind::NewIdSpecific => {
			// let nulterm = p
			// 	.iter()
			// 	.enumerate()
			// 	.find(|(_, c)| **c == b'\0')
			// 	.map(|(e, _)| e);
			// if let Some(pos) = nulterm {
			// 	let slice = &p[0..pos];
			// 	let str_ = str::from_utf8(slice)?;
			// 	let version = u32::from_ne_bytes([p[pos], p[pos + 1], p[pos + 2], p[pos + 3]]);
			// 	let new_id = u32::from_ne_bytes([p[pos + 4], p[pos + 5], p[pos + 6], p[pos + 7]]);
			// 	Ok(WireArgument::NewIdSpecific(
			// 		str_.to_string(),
			// 		version,
			// 		new_id,
			// 	))
			// } else {
			// 	Err(())
			// }
			todo!()
		}
		WireArgumentKind::Arr => Ok(WireArgument::Arr(payload.to_vec())),
	}
}
