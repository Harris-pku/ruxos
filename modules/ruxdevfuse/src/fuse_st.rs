/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

#![warn(dead_code)]

use log::*;

#[derive(Debug, Clone, Copy)]
pub enum FuseOpcode {
	FuseLookup		= 1,
	FuseForget		= 2,  /* no reply */
	FuseGetattr		= 3,
	FuseSetattr		= 4,
	FuseReadlink	= 5,
	FuseSymlink		= 6,
	FuseMknod		= 8,
	FuseMkdir		= 9,
	FuseUnlink		= 10,
	FuseRmdir		= 11,
	FuseRename		= 12,
	FuseLink		= 13,
	FuseOpen		= 14,
	FuseRead		= 15,
	FuseWrite		= 16,
	FuseStatfs		= 17,
	FuseRelease		= 18,
	FuseFsync		= 20,
	FuseSetxattr		= 21,
	FuseGetxattr		= 22,
	FuseListxattr		= 23,
	FuseRemovexattr		= 24,
	FuseFlush		    = 25,
	FuseInit		    = 26,
	FuseOpendir			= 27,
	FuseReaddir			= 28,
	FuseReleasedir		= 29,
	FuseFsyncdir		= 30,
	FuseGetlk		    = 31,
	FuseSetlk		    = 32,
	FuseSetlkw		    = 33,
	FuseAccess		    = 34,
	FuseCreate		    = 35,
	FuseInterrupt	    = 36,
	FuseBmap		    = 37,
	FuseDestroy	    	= 38,
	FuseIoctl		    = 39,
	FusePoll		    = 40,
	FuseNotifyReply		= 41,
	FuseBatchForget		= 42,
	FuseFallocate		= 43,
	FuseReaddirplus		= 44,
	FuseRename2			= 45,
	FuseLseek		    = 46,
	FuseCopyFileRange	= 47,
	FuseSetupmapping	= 48,
	FuseRemovemapping	= 49,
	FuseSyncfs		    = 50,
	FuseTmpfile			= 51,
}

#[derive(Debug, Clone, Copy)]
pub struct FuseInHeader {
    len: u32,     // length of the request = sizeof(fuse_in_header) = 32
    opcode: u32,  // eg. FUSE_GETATTR = 3
    unique: u64,  // unique request ID
    nodeid: u64,  // inode number
    uid: u32,     // user ID
    gid: u32,     // group ID
    pid: u32,     // process ID
    padding: u32, // padding
}

impl FuseInHeader {
    pub fn new(len: u32, opcode: u32, unique: u64, nodeid: u64, uid: u32, gid: u32, pid: u32, padding: u32) -> Self {
        Self {
            len,
            opcode,
            unique,
            nodeid,
            uid,
            gid,
            pid,
            padding,
        }
    }

	pub fn print(&self) {
		info!("FuseInHeader: len: {:?}, opcode: {:?}, unique: {:?}, nodeid: {:?}, uid: {:?}, gid: {:?}, pid: {:?}, padding: {:?}", self.len, self.opcode, self.unique, self.nodeid, self.uid, self.gid, self.pid, self.padding);
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.len.to_le_bytes());
		buf[4..8].copy_from_slice(&self.opcode.to_le_bytes());
		buf[8..16].copy_from_slice(&self.unique.to_le_bytes());
		buf[16..24].copy_from_slice(&self.nodeid.to_le_bytes());
		buf[24..28].copy_from_slice(&self.uid.to_le_bytes());
		buf[28..32].copy_from_slice(&self.gid.to_le_bytes());
		buf[32..36].copy_from_slice(&self.pid.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
pub struct FuseOutHeader {
    len: u32,     // length of the response
    error: i32,   // error code
    unique: u64,  // unique request ID
}

impl FuseOutHeader {
    pub fn new(len: u32, error: i32, unique: u64) -> Self {
        Self {
            len,
            error,
            unique,
        }
    }

	pub fn read_from(buf: &[u8]) -> Self {
		Self {
			len: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
			error: i32::from_le_bytes(buf[4..8].try_into().unwrap()),
			unique: u64::from_le_bytes(buf[8..16].try_into().unwrap()),
		}
	}
	pub fn print(&self) {
		info!("fuse_out_header: len: {:?}, error: {:?}, unique: {:?}", self.len, self.error, self.unique);
	}
    
	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.len.to_le_bytes());
		buf[4..8].copy_from_slice(&self.error.to_le_bytes());
		buf[8..16].copy_from_slice(&self.unique.to_le_bytes());
	}
}


#[derive(Debug, Clone, Copy)]
pub struct FuseInitIn {
	major: u32,
	minor: u32,
    max_readahead: u32,
    flags: u32,
    flags2: u32,
    unused: [u32; 11],
}

impl FuseInitIn {
    pub fn new(major: u32, minor: u32, max_readahead: u32, flags: u32, flags2: u32, unused: [u32; 11]) -> Self {
        Self {
            major,
            minor,
            max_readahead,
            flags,
            flags2,
            unused,
        }
    }

	pub fn print(&self) {
		info!("FuseInitIn: major: {:?}, minor: {:?}, max_readahead: {:?}, flags: {:?}, flags2: {:?}, unused: {:?}", self.major, self.minor, self.max_readahead, self.flags, self.flags2, self.unused);
	}

    pub fn write_to(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&self.major.to_le_bytes());
        buf[4..8].copy_from_slice(&self.minor.to_le_bytes());
        buf[8..12].copy_from_slice(&self.max_readahead.to_le_bytes());
        buf[12..16].copy_from_slice(&self.flags.to_le_bytes());
        buf[16..20].copy_from_slice(&self.flags2.to_le_bytes());
		for (i, &val) in self.unused.iter().enumerate() {
			buf[20 + i * 4..24 + i * 4].copy_from_slice(&val.to_le_bytes());
		}
        // buf[20..52].copy_from_slice(&self.unused.to_le_bytes());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FuseInitOut {
	major: u32,
	minor: u32,
	max_readahead: u32,
	flags: u32,
	max_background: u16,
	congestion_threshold: u16,
	max_write: u32,
	time_gran: u32,
	max_pages: u16,
	map_alignment: u16,
	flags2: u32,
	unused: [u32; 7],
}

impl FuseInitOut {
	pub fn new(major: u32, minor: u32, max_readahead: u32, flags: u32, max_background: u16, congestion_threshold: u16, max_write: u32, time_gran: u32, max_pages: u16, map_alignment: u16, flags2: u32, unused: [u32; 7]) -> Self {
		Self {
			major,
			minor,
			max_readahead,
			flags,
			max_background,
			congestion_threshold,
			max_write,
			time_gran,
			max_pages,
			map_alignment,
			flags2,
			unused,
		}
	}

	pub fn read_from(buf: &[u8]) -> Self {
		info!("fuseinitout len: {:?}, buf: {:?}", buf.len(), buf);
		Self {
			major: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
			minor: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
			max_readahead: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
			flags: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
			max_background: u16::from_le_bytes(buf[16..18].try_into().unwrap()),
			congestion_threshold: u16::from_le_bytes(buf[18..20].try_into().unwrap()),
			max_write: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
			time_gran: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
			max_pages: u16::from_le_bytes(buf[28..30].try_into().unwrap()),
			map_alignment: u16::from_le_bytes(buf[30..32].try_into().unwrap()),
			flags2: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
			unused: [
				u32::from_le_bytes(buf[36..40].try_into().unwrap()),
				u32::from_le_bytes(buf[40..44].try_into().unwrap()),
				u32::from_le_bytes(buf[44..48].try_into().unwrap()),
				u32::from_le_bytes(buf[48..52].try_into().unwrap()),
				u32::from_le_bytes(buf[52..56].try_into().unwrap()),
				u32::from_le_bytes(buf[56..60].try_into().unwrap()),
				u32::from_le_bytes(buf[60..64].try_into().unwrap()),
			],
		}
	}

	pub fn print(&self) {
		info!("FuseInitOut: major: {:?}, minor: {:?}, max_readahead: {:?}, flags: {:?}, max_background: {:?}, congestion_threshold: {:?}, max_write: {:?}, time_gran: {:?}, max_pages: {:?}, map_alignment: {:?}, flags2: {:?}, unused: {:?}", self.major, self.minor, self.max_readahead, self.flags, self.max_background, self.congestion_threshold, self.max_write, self.time_gran, self.max_pages, self.map_alignment, self.flags2, self.unused);
	}
}

#[derive(Debug, Clone, Copy)]
pub struct FuseGetattrIn {
	getattr_flags: u32,
	dummy: u32,
	fh: u64,
}

impl FuseGetattrIn {
	pub fn new(getattr_flags: u32, dummy: u32, fh: u64) -> Self {
		Self {
			getattr_flags,
			dummy,
			fh,
		}
	}

	pub fn print(&self) {
		info!("FuseGetattrIn: getattr_flags: {:?}, dummy: {:?}, fh: {:?}", self.getattr_flags, self.dummy, self.fh);
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.getattr_flags.to_le_bytes());
		buf[4..8].copy_from_slice(&self.dummy.to_le_bytes());
		buf[8..16].copy_from_slice(&self.fh.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
pub struct FuseAttr {
	ino: u64,
	size: u64,
	blocks: u64,
	atime: u64,
	mtime: u64,
	ctime: u64,
	crtime: u64,
	atimensec: u32,
	mtimensec: u32,
	ctimensec: u32,
	crtimensec: u32,
	mode: u32,
	nlink: u32,
	uid: u32,
	gid: u32,
	rdev: u32,
	blksize: u32,
	padding: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct FuseAttrOut {
	attr_valid: u64,
	attr_valid_nsec: u32,
	dummy: u32,
	attr: FuseAttr,
}

#[derive(Debug, Clone, Copy)]
pub struct FuseOpenIn {
	flags: u32,
	open_flags: u32,	/* FUSE_OPEN_... */
}

impl FuseOpenIn {
	pub fn new(flags: u32, open_flags: u32) -> Self {
		Self {
			flags,
			open_flags,
		}
	}

	pub fn print(&self) {
		info!("FuseOpenIn: flags: {:?}, open_flags: {:?}", self.flags, self.open_flags);
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.flags.to_le_bytes());
		buf[4..8].copy_from_slice(&self.open_flags.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
pub struct FuseOpenOut {
	fh: u64,
	open_flags: u32,
	padding: u32,
}

impl FuseOpenOut {
	pub fn new(fh: u64, open_flags: u32) -> Self {
		Self {
			fh,
			open_flags,
			padding: 0,
		}
	}

	pub fn read_from(buf: &[u8]) -> Self {
		Self {
			fh: u64::from_le_bytes(buf[0..8].try_into().unwrap()),
			open_flags: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
			padding: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
		}
	}

	pub fn print(&self) {
		info!("FuseOpenOut: fh: {:?}, open_flags: {:?}, padding: {:?}", self.fh, self.open_flags, self.padding);
	}
}

#[derive(Debug, Clone, Copy)]
pub struct FuseReadIn {
	fh: u64,
	offset: u64,
	size: u32,
	read_flags: u32,
	lock_owner: u64,
	flags: u32,
	padding: u32,
}

impl FuseReadIn {
	pub fn new(fh: u64, offset: u64, size: u32, read_flags: u32, lock_owner: u64, flags: u32) -> Self {
		Self {
			fh,
			offset,
			size,
			read_flags,
			lock_owner,
			flags,
			padding: 0,
		}
	}

	pub fn print(&self) {
		info!("FuseReadIn: fh: {:?}, offset: {:?}, size: {:?}, read_flags: {:?}, lock_owner: {:?}, flags: {:?}, padding: {:?}", self.fh, self.offset, self.size, self.read_flags, self.lock_owner, self.flags, self.padding);
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..8].copy_from_slice(&self.fh.to_le_bytes());
		buf[8..16].copy_from_slice(&self.offset.to_le_bytes());
		buf[16..20].copy_from_slice(&self.size.to_le_bytes());
		buf[20..24].copy_from_slice(&self.read_flags.to_le_bytes());
		buf[24..32].copy_from_slice(&self.lock_owner.to_le_bytes());
		buf[32..36].copy_from_slice(&self.flags.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
struct FuseWriteIn {
	fh: u64,
	offset: u64,
	size: u32,
	write_flags: u32,
	lock_owner: u64,
	flags: u32,
	padding: u32,
}

impl FuseWriteIn {
	pub fn new(fh: u64, offset: u64, size: u32, write_flags: u32, lock_owner: u64, flags: u32) -> Self {
		Self {
			fh,
			offset,
			size,
			write_flags,
			lock_owner,
			flags,
			padding: 0,
		}
	}

	pub fn print(&self) {
		info!("FuseWriteIn: fh: {:?}, offset: {:?}, size: {:?}, write_flags: {:?}, lock_owner: {:?}, flags: {:?}, padding: {:?}", self.fh, self.offset, self.size, self.write_flags, self.lock_owner, self.flags, self.padding);
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..8].copy_from_slice(&self.fh.to_le_bytes());
		buf[8..16].copy_from_slice(&self.offset.to_le_bytes());
		buf[16..20].copy_from_slice(&self.size.to_le_bytes());
		buf[20..24].copy_from_slice(&self.write_flags.to_le_bytes());
		buf[24..32].copy_from_slice(&self.lock_owner.to_le_bytes());
		buf[32..36].copy_from_slice(&self.flags.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
pub struct FuseWriteOut {
	size: u32,
	padding: u32,
}

impl FuseWriteOut {
	pub fn new(size: u32) -> Self {
		Self {
			size,
			padding: 0,
		}
	}

	pub fn read_from(buf: &[u8]) -> Self {
		Self {
			size: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
			padding: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
		}
	}

	pub fn print(&self) {
		info!("FuseWriteOut: size: {:?}, padding: {:?}", self.size, self.padding);
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.size.to_le_bytes());
	}
}