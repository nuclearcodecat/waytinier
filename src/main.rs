#![allow(dead_code)]

use std::env;

use crate::wayland::{Display, MessageManager, Registry};

mod wayland {
	use std::{env, io::{Read, Write}, os::unix::net::UnixStream, path::PathBuf};

	pub struct Registry {
		pub id: u32,
	}

	pub struct Display {
		pub id: u32,
	}

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
		NewIdSpecific(String, u32, u32),
		Arr(Vec<u8>),
		FileDescriptor(u32),
	}

	pub struct MessageManager {
		pub sock: UnixStream,
		pub last_ass_id: u32,
	}

	impl MessageManager {
		pub fn new(sockname: &str) -> Result<Self, ()> {
			let base = env::var("XDG_RUNTIME_DIR").map_err(|_| {})?; 
			let mut base = PathBuf::from(base);
			base.push(sockname);
			let sock = UnixStream::connect(base).map_err(|_| {})?;
			sock.set_nonblocking(true).map_err(|_| {})?;
			let wlmm = Self {
				sock,
				last_ass_id: 1,
			};

			Ok(wlmm)
		}

		pub fn discon(&self) -> Result<(), ()> {
			self.sock.shutdown(std::net::Shutdown::Both).map_err(|_| {})
		}

		fn increment_id(&mut self) {
			self.last_ass_id += 1;
		}

		fn send_request(&mut self, msg: &mut WireMessage) -> Result<(), ()> {
			let mut buf: Vec<u8> = vec![];
			buf.append(&mut Vec::from(msg.sender_id.to_ne_bytes()));
			let argsize = {
				// header is 8
				let mut complete = 8;
				for n in msg.args.iter() {
					let size = n.size();
					complete += size;
				}
				complete
			};
			let word2 = (argsize << 16) as u32 | (msg.opcode as u32 & 0x0000ffffu32);
			buf.append(&mut Vec::from(word2.to_ne_bytes()));
			for obj in msg.args.iter_mut() {
				match obj {
					WireArgument::Arr(x) => buf.append(x),
					_ => buf.append(&mut obj.as_vec_u8())
				}
			}
			self.sock.write_all(&buf).map_err(|_| {})?;
			Ok(())
		}

		pub fn get_event(&mut self) -> Result<Option<Vec<WireMessage>>, ()> {
			let mut b: Vec<u8> = vec![];
			let len;
			match self.sock.read_to_end(&mut b) {
				Ok(l) => {
					len = l;
					println!("==== read to end\n{:?}", b);
					if let Ok(str_) = str::from_utf8(&b) {
						println!("==== string conversion\n{}", str_);
					} else {
						eprintln!("string conversion failed");
					}
				},
				Err(er) => {
					eprintln!("er: {:#?}", er);
					match er.kind() {
						std::io::ErrorKind::WouldBlock => return Ok(None),
						_ => {
							return Err(());
						},
					}
				},
			}

			let mut events = vec![];
			let mut cursor = 0;
			let mut cursor_last = 0;
			let mut ctr = 0;
			while cursor < len {
				let sender_id = u32::from_ne_bytes([b[cursor], b[cursor + 1], b[cursor + 2], b[cursor + 3]]);
				let byte2 = u32::from_ne_bytes([b[cursor + 4], b[cursor + 5], b[cursor + 6], b[cursor + 7]]);

				let recv_len = byte2 >> 16;
				println!("len: {}", recv_len);
				if recv_len < 8 {
					eprintln!("recv_len bad");
					return Err(());
				}
				let opcode = (byte2 & 0x0000ffff) as usize;

				println!("==== iter {}\nsender_id: {}, recv_len: {}, opcode: {}, payload ->:\n{:?}", ctr, sender_id, recv_len, opcode, &b[cursor_last + 8..cursor_last + recv_len as usize]);

				let event = WireMessage {
					sender_id,
					opcode,
					args: vec![],
				};
				events.push(event);

				cursor = cursor_last + recv_len as usize;
				ctr += 1;
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
				WireArgument::String(x) => Vec::from(x.as_str()),
				WireArgument::Obj(x) => Vec::from(x.to_ne_bytes()),
				WireArgument::NewId(x) => Vec::from(x.to_ne_bytes()),
				WireArgument::NewIdSpecific(x, y, z) => {
					let mut complete: Vec<u8> = vec![];
					complete.append(&mut Vec::from(x.as_str()));
					complete.append(&mut Vec::from(y.to_ne_bytes()));
					complete.append(&mut Vec::from(z.to_ne_bytes()));
					complete
				},
				WireArgument::Arr(items) => items.clone(),
				WireArgument::FileDescriptor(x) => Vec::from(x.to_ne_bytes()),
			}
		}
	}

	impl Display {
		pub fn new() -> Self {
			Self {
				id: 1,
			}
		}

		pub fn wl_get_registry(&mut self, wlmm: &mut MessageManager) -> Result<u32, ()> {
			wlmm.increment_id();
			wlmm.send_request(&mut WireMessage {
				sender_id: self.id,
				// second request in the proto
				opcode: 1,
				args: vec![
					// wl_registry id is now 2 since 1 is the display
					WireArgument::NewId(wlmm.last_ass_id),
				],
			})?;
			Ok(wlmm.last_ass_id)
		}

		pub fn wl_sync(&mut self, wlmm: &mut MessageManager) -> Result<(), ()> {
			wlmm.increment_id();
			wlmm.send_request(&mut WireMessage {
				sender_id: self.id,
				opcode: 0,
				args: vec![
					WireArgument::NewId(wlmm.last_ass_id),
				],
			})
		}
	}

	impl Registry {
		pub fn new(id: u32) -> Self {
			Self {
				id,
			}
		}

		pub fn wl_bind(&mut self, wlmm: &mut MessageManager) -> Result<(), ()> {
			wlmm.increment_id();
			wlmm.send_request(&mut WireMessage {
				// wl_registry id
				sender_id: self.id,
				// first request in the proto
				opcode: 0,
				args: vec![
					WireArgument::UnInt(self.id),
					WireArgument::NewId(wlmm.last_ass_id),
				],
			})
		}
	}
}

fn main() -> Result<(), ()> {
	let display_name = env::var("WAYLAND_DISPLAY").map_err(|_| {})?;
	let mut wlmm = MessageManager::new(&display_name)?;
	let mut display = Display::new();
	let reg_id = display.wl_get_registry(&mut wlmm)?;
	println!("reg id: {}", reg_id);
	display.wl_sync(&mut wlmm)?;
	let mut registry = Registry::new(reg_id);
	registry.wl_bind(&mut wlmm)?;

	// let mut buf = String::new();
	// let _ = wlmm.sock.read_to_string(&mut buf).unwrap();
	// println!("read str: {}", buf);
	
	let mut read = wlmm.get_event()?;
	while read.is_none() {
		read = wlmm.get_event()?;
	}
	println!("\n\n==== EVENT\n{:#?}", read);

	wlmm.discon()?;
	println!("good");
	Ok(())
}
