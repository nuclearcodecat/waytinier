// /* ioctl command encoding: 32 bits total, command in lower 16 bits,
//  * size of the parameter structure in the lower 14 bits of the
//  * upper 16 bits.
//  * Encoding the size of the parameter structure in the ioctl request
//  * is useful for catching programs compiled with old versions
//  * and to avoid overwriting user space outside the user buffer area.
//  * The highest 2 bits are reserved for indicating the ``access mode''.
//  * NOTE: This limits the max parameter size to 16kB -1 !
//  */
// /*
//  * The following is for compatibility across the various Linux
//  * platforms.  The generic ioctl numbering scheme doesn't really enforce
//  * a type field.  De facto, however, the top 8 bits of the lower 16
//  * bits are indeed used as a type field, so we might just as well make
//  * this explicit here.  Please be sure to use the decoding macros
//  * below from now on.
//  */
// #define _IOC_NRBITS	8
// #define _IOC_TYPEBITS	8

// /*
//  * Let any architecture override either of the following before
//  * including this file.
//  */
// #ifndef _IOC_SIZEBITS
// # define _IOC_SIZEBITS	14
// #endif

// #ifndef _IOC_DIRBITS
// # define _IOC_DIRBITS	2
// #endif

// #define _IOC_NRMASK	((1 << _IOC_NRBITS)-1)
// #define _IOC_TYPEMASK	((1 << _IOC_TYPEBITS)-1)
// #define _IOC_SIZEMASK	((1 << _IOC_SIZEBITS)-1)
// #define _IOC_DIRMASK	((1 << _IOC_DIRBITS)-1)

// #define _IOC_NRSHIFT	0
// #define _IOC_TYPESHIFT	(_IOC_NRSHIFT+_IOC_NRBITS)
// #define _IOC_SIZESHIFT	(_IOC_TYPESHIFT+_IOC_TYPEBITS)
// #define _IOC_DIRSHIFT	(_IOC_SIZESHIFT+_IOC_SIZEBITS)

// /*
//  * Direction bits, which any architecture can choose to override
//  * before including this file.
//  *
//  * NOTE: _IOC_WRITE means userland is writing and kernel is
//  * reading. _IOC_READ means userland is reading and kernel is writing.
//  */
// #ifndef _IOC_NONE
// # define _IOC_NONE	0U
// #endif

// #ifndef _IOC_WRITE
// # define _IOC_WRITE	1U
// #endif

// #ifndef _IOC_READ
// # define _IOC_READ	2U
// #endif

// #define _IOC(dir,type,nr,size) \
// 	(((dir)  << _IOC_DIRSHIFT) | \
// 	 ((type) << _IOC_TYPESHIFT) | \
// 	 ((nr)   << _IOC_NRSHIFT) | \
// 	 ((size) << _IOC_SIZESHIFT))

// #ifndef __KERNEL__
// #define _IOC_TYPECHECK(t) (sizeof(t))
// #endif

// /*
//  * Used to create numbers.
//  *
//  * NOTE: _IOW means userland is writing and kernel is reading. _IOR
//  * means userland is reading and kernel is writing.
//  */
// #define _IO(type,nr)			_IOC(_IOC_NONE,(type),(nr),0)
// #define _IOR(type,nr,argtype)		_IOC(_IOC_READ,(type),(nr),(_IOC_TYPECHECK(argtype)))
// #define _IOW(type,nr,argtype)		_IOC(_IOC_WRITE,(type),(nr),(_IOC_TYPECHECK(argtype)))
// #define _IOWR(type,nr,argtype)		_IOC(_IOC_READ|_IOC_WRITE,(type),(nr),(_IOC_TYPECHECK(argtype)))
// #define _IOR_BAD(type,nr,argtype)	_IOC(_IOC_READ,(type),(nr),sizeof(argtype))
// #define _IOW_BAD(type,nr,argtype)	_IOC(_IOC_WRITE,(type),(nr),sizeof(argtype))
// #define _IOWR
//

const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_DIRBITS: u32 = 2;

const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;

const fn ioc(dir: u32, type_: u32, nr: u32, size: u32) -> u32 {
	dir << IOC_DIRSHIFT | type_ << IOC_TYPESHIFT | nr << IOC_NRSHIFT | size << IOC_SIZESHIFT
}

const fn iowr<T>(type_: u32, nr: u32) -> u32 {
	ioc(IOC_READ | IOC_WRITE, type_, nr, std::mem::size_of::<T>() as u32)
}

const fn drm_iowr<T>(nr: u32) -> u32 {
	iowr::<T>(b'd' as u32, nr)
}

// /**
//  * struct drm_mode_create_dumb - Create a KMS dumb buffer for scanout.
//  * @height: buffer height in pixels
//  * @width: buffer width in pixels
//  * @bpp: bits per pixel
//  * @flags: must be zero
//  * @handle: buffer object handle
//  * @pitch: number of bytes between two consecutive lines
//  * @size: size of the whole buffer in bytes
//  *
//  * User-space fills @height, @width, @bpp and @flags. If the IOCTL succeeds,
//  * the kernel fills @handle, @pitch and @size.
//  */
// struct drm_mode_create_dumb {
// 	__u32 height;
// 	__u32 width;
// 	__u32 bpp;
// 	__u32 flags;

// 	__u32 handle;
// 	__u32 pitch;
// 	__u64 size;
// };
#[repr(C)]
#[derive(Default)]
pub(crate) struct ModeCreateDumb {
	pub(crate) height: u32,
	pub(crate) width: u32,
	pub(crate) bpp: u32,
	pub(crate) flags: u32,
	pub(crate) handle: u32,
	pub(crate) pitch: u32,
	pub(crate) size: u64,
}

pub(crate) const DRM_IOCTL_MODE_CREATE_DUMB: u32 = drm_iowr::<ModeCreateDumb>(0xb2);
