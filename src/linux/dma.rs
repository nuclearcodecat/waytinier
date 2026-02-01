use std::os::fd::RawFd;

use crate::linux::ioctl::{DRM_IOCTL_MODE_CREATE_DUMB, ModeCreateDumb};

pub(crate) const fn fourcc_code(a: u8, b: u8, c: u8, d: u8) -> u32 {
	let a = a as u32;
	let b = b as u32;
	let c = c as u32;
	let d = d as u32;
	(a | b << 8) | (c << 16) | (d << 24)
}

// https://github.com/torvalds/linux/blob/master/include/uapi/drm/drm_fourcc.h line 467
#[repr(u64)]
pub(crate) enum DrmFormatModVendor {
	None = 0,
}

pub(crate) const fn fourcc_mod_code(vendor: DrmFormatModVendor, val: u64) -> u64 {
	(vendor as u64) << 56 | val & 0x00ffffffffffffff
}

pub(crate) const DRM_FORMAT_MOD_LINEAR: u64 = fourcc_mod_code(DrmFormatModVendor::None, 0);

pub(crate) fn make_dumb_buffer(
	fd: RawFd,
	width: u32,
	height: u32,
	bpp: u32,
) -> Result<ModeCreateDumb, std::io::Error> {
	let mut mode_create_dumb = ModeCreateDumb {
		width,
		height,
		bpp,
		flags: 0,
		..Default::default()
	};

	let ret = unsafe { libc::ioctl(fd, DRM_IOCTL_MODE_CREATE_DUMB as u64, &mut mode_create_dumb) };
	if ret == -1 {
		return Err(std::io::Error::last_os_error());
	}
	Ok(mode_create_dumb)
}
