use std::error::Error;

use waytinier::abstraction::app::{App, BufferBackendKind, TopLevelWindow};

struct AppState {}

fn main() -> Result<(), Box<dyn Error>> {
	let mut app = App::new()?;
	let window = TopLevelWindow::spawner(&mut app)
		.with_buffer_backend(BufferBackendKind::SharedMemory)
		.spawn()?;
	let _ = app.push_presenter(window)?;

	let mut state = AppState {};
	#[allow(clippy::never_loop)]
	loop {
		app.work(&mut state, |_state, ss| {
			for y in 0..ss.h as usize {
				for x in 0..ss.w as usize {
					let ix = (ss.w as usize * y + x) * ss.pf.width() as usize;
					ss.buf[ix] = 0xff;
					ss.buf[ix + 1] = 0xe4;
					ss.buf[ix + 2] = 0xff;
					ss.buf[ix + 3] = 0xff;
				}
			}
		})?;
		break;
	}
	Ok(())
}
