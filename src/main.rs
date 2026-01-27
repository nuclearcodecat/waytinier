#![allow(dead_code)]
use std::{
	error::Error,
	fs::File,
	io::{BufRead, BufReader, Read},
};

use wayland_raw::abstraction::app::{App, TopLevelWindow};

struct AppState {
	img_w: usize,
	img_h: usize,
	machine: Vec<u8>,
}

fn main() -> Result<(), Box<dyn Error>> {
	let mut app = App::new()?;
	let window = TopLevelWindow::spawner(&mut app).spawn()?;
	app.push_presenter(window)?;

	let (img_w, img_h, machine) = parse_pix("pix.ppm")?;
	let mut state = AppState {
		img_w,
		img_h,
		machine,
	};

	loop {
		let done = app.work(&mut state, |state, ss| {
			let (r, g, b) = hsv_to_rgb((ss.frame % 360) as f64, 1.0, 1.0);
			let start_x = ss.w as isize / 2 - state.img_w as isize / 2;
			let start_y = ss.h as isize / 2 - state.img_h as isize / 2;

			for y in 0..ss.h as usize {
				for x in 0..ss.w as usize {
					let surface_ix = (ss.w as usize * y + x) * 4;

					let rel_x = x as isize - start_x;
					let rel_y = y as isize - start_y;

					if rel_x >= 0
						&& rel_x < state.img_w as isize
						&& rel_y >= 0 && rel_y < state.img_h as isize
					{
						let img_ix = (rel_y as usize * img_w + rel_x as usize) * 3;
						ss.buf[surface_ix + 2] = state.machine[img_ix];
						ss.buf[surface_ix + 1] = state.machine[img_ix + 1];
						ss.buf[surface_ix] = state.machine[img_ix + 2];
					} else {
						ss.buf[surface_ix] = b.wrapping_sub(x as u8);
						ss.buf[surface_ix + 1] = g.wrapping_add(y as u8);
						ss.buf[surface_ix + 2] = r.wrapping_shl(x as u32);
					}
				}
			}
		})?;
		if done {
			break;
		}
	}
	Ok(())
}

fn parse_pix(path: &str) -> Result<(usize, usize, Vec<u8>), Box<dyn Error>> {
	let file = File::open(path)?;
	let mut rd = BufReader::new(file);

	let mut buf = String::new();
	rd.read_line(&mut buf)?;
	if buf.trim_end() != "P6" {
		// println!("{:?}", unsafe { buf.as_mut_vec() });
		return Err("file format not ppm".into());
	}

	buf.clear();
	rd.read_line(&mut buf)?;
	let mut nonws = buf.split_whitespace();
	let w = nonws.next().ok_or("parsing w failed")?.parse()?;
	let h = nonws.next().ok_or("parsing h failed")?.parse()?;

	rd.skip_until(b'\n')?;

	let mut raster: Vec<u8> = vec![];
	rd.read_to_end(&mut raster)?;

	Ok((w, h, raster))
}

// stolen from hsv library
#[allow(dead_code)]
pub fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (u8, u8, u8) {
	fn is_between(value: f64, min: f64, max: f64) -> bool {
		min <= value && value < max
	}

	// check_bounds(hue, saturation, value);

	let c = value * saturation;
	let h = hue / 60.0;
	let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
	let m = value - c;

	let (r, g, b): (f64, f64, f64) = if is_between(h, 0.0, 1.0) {
		(c, x, 0.0)
	} else if is_between(h, 1.0, 2.0) {
		(x, c, 0.0)
	} else if is_between(h, 2.0, 3.0) {
		(0.0, c, x)
	} else if is_between(h, 3.0, 4.0) {
		(0.0, x, c)
	} else if is_between(h, 4.0, 5.0) {
		(x, 0.0, c)
	} else {
		(c, 0.0, x)
	};

	(((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
}
