/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */


#[derive(Debug, Clone, Copy)]
pub enum fuse_opcode {
    FUSE_LOOKUP		= 1,
	FUSE_FORGET		= 2,  /* no reply */
	FUSE_GETATTR	= 3,
	FUSE_SETATTR	= 4,
	FUSE_READLINK	= 5,
	FUSE_SYMLINK	= 6,
	FUSE_MKNOD		= 8,
	FUSE_MKDIR		= 9,
	FUSE_UNLINK		= 10,
	FUSE_RMDIR		= 11,
	FUSE_RENAME		= 12,
	FUSE_LINK		= 13,
	FUSE_OPEN		= 14,
	FUSE_READ		= 15,
	FUSE_WRITE		= 16,
	FUSE_STATFS		= 17,
	FUSE_RELEASE	= 18,
	FUSE_FSYNC		= 20,
	FUSE_SETXATTR		= 21,
	FUSE_GETXATTR		= 22,
	FUSE_LISTXATTR		= 23,
	FUSE_REMOVEXATTR	= 24,
	FUSE_FLUSH		    = 25,
	FUSE_INIT		    = 26,
	FUSE_OPENDIR		= 27,
	FUSE_READDIR		= 28,
	FUSE_RELEASEDIR		= 29,
	FUSE_FSYNCDIR		= 30,
	FUSE_GETLK		    = 31,
	FUSE_SETLK		    = 32,
	FUSE_SETLKW		    = 33,
	FUSE_ACCESS		    = 34,
	FUSE_CREATE		    = 35,
	FUSE_INTERRUPT	    = 36,
	FUSE_BMAP		    = 37,
	FUSE_DESTROY	    = 38,
	FUSE_IOCTL		    = 39,
	FUSE_POLL		    = 40,
	FUSE_NOTIFY_REPLY	= 41,
	FUSE_BATCH_FORGET	= 42,
	FUSE_FALLOCATE		= 43,
	FUSE_READDIRPLUS	= 44,
	FUSE_RENAME2		= 45,
	FUSE_LSEEK		    = 46,
	FUSE_COPY_FILE_RANGE	= 47,
	FUSE_SETUPMAPPING	= 48,
	FUSE_REMOVEMAPPING	= 49,
	FUSE_SYNCFS		    = 50,
	FUSE_TMPFILE		= 51,
}

#[derive(Debug, Clone, Copy)]
pub struct fuse_in_header {
    len: u32,     // length of the request = sizeof(fuse_in_header) = 32
    opcode: u32,  // eg. FUSE_GETATTR = 3
    unique: u64,  // unique request ID
    nodeid: u64,  // inode number
    uid: u32,     // user ID
    gid: u32,     // group ID
    pid: u32,     // process ID
    padding: u32, // padding
}

impl fuse_in_header {
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
pub struct fuse_open_in {
	flags: u32,
	open_flags: u32,	/* FUSE_OPEN_... */
}

impl fuse_open_in {
	pub fn new(flags: u32, open_flags: u32) -> Self {
		Self {
			flags,
			open_flags,
		}
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.flags.to_le_bytes());
		buf[4..8].copy_from_slice(&self.open_flags.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
pub struct fuse_open_out {
	fh: u64,
	open_flags: u32,
	padding: u32,
}

impl fuse_open_out {
	pub fn new(fh: u64, open_flags: u32) -> Self {
		Self {
			fh,
			open_flags,
			padding: 0,
		}
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..8].copy_from_slice(&self.fh.to_le_bytes());
		buf[8..12].copy_from_slice(&self.open_flags.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
pub struct fuse_read_in {
	fh: u64,
	offset: u64,
	size: u32,
	read_flags: u32,
	lock_owner: u64,
	flags: u32,
	padding: u32,
}

impl fuse_read_in {
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
struct fuse_write_in {
	fh: u64,
	offset: u64,
	size: u32,
	write_flags: u32,
	lock_owner: u64,
	flags: u32,
	padding: u32,
}

impl fuse_write_in {
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
pub struct fuse_out_header {
    len: u32,     // length of the response
    error: i32,   // error code
    unique: u64,  // unique request ID
}

impl fuse_out_header {
    pub fn new(len: u32, error: i32, unique: u64) -> Self {
        Self {
            len,
            error,
            unique,
        }
    }
    
	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.len.to_le_bytes());
		buf[4..8].copy_from_slice(&self.error.to_le_bytes());
		buf[8..16].copy_from_slice(&self.unique.to_le_bytes());
	}
}

#[derive(Debug, Clone, Copy)]
pub struct fuse_write_out {
	size: u32,
	padding: u32,
}

impl fuse_write_out {
	pub fn new(size: u32) -> Self {
		Self {
			size,
			padding: 0,
		}
	}

	pub fn write_to(&self, buf: &mut [u8]) {
		buf[0..4].copy_from_slice(&self.size.to_le_bytes());
	}
}