/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

// #![cfg(feature = "multitask")]

use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::string::String;
use alloc::vec::Vec;
use ruxtask::current;
use spinlock::SpinNoIrq;
use core::sync::atomic::{AtomicI32, Ordering};
use log::*;

use axfs_vfs::{VfsDirEntry, VfsError, VfsResult};
use axfs_vfs::{VfsNodeAttr, VfsNodeOps, VfsNodeRef, VfsNodeType, VfsOps};
use spin::{once::Once, RwLock};
use ruxfs::fuse_st::{
    FuseAccessIn, FuseAttr, FuseAttrOut, FuseCreateIn, FuseDirent, FuseEntryOut, FuseFlushIn, FuseGetattrIn, FuseInHeader, FuseInitIn, FuseInitOut, FuseMkdirIn, FuseMknodIn, FuseOpcode, FuseOpenIn, FuseOpenOut, FuseOutHeader, FuseReadIn, FuseReleaseIn, FuseRename2In, FuseRenameIn, FuseWriteIn, FuseWriteOut
};
use ruxfs::devfuse::{FUSEFLAG, FUSE_VEC};

pub static mut UNIQUE_ID: u64 = 0;
pub static INITFLAG: AtomicI32 = AtomicI32::new(1);

/// It implements [`axfs_vfs::VfsOps`].
pub struct FuseFS {
    parent: Once<VfsNodeRef>,
    root: Arc<FuseNode>,
}

impl FuseFS {
    /// Create a new instance.
    pub fn new() -> Self {
        info!("fusefs new...");
        // let parent: Weak<dyn VfsNodeOps> = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
        Self {
            parent: Once::new(),
            root: FuseNode::new(None, 1, FuseAttr::default(), 0, 0),
        }
    }

    /// Add a node to the root directory.
    ///
    /// The node must implement [`axfs_vfs::VfsNodeOps`], and be wrapped in [`Arc`].
    pub fn add(&self, name: &'static str, node: VfsNodeRef) {
        info!("fusefs add...");
        self.root.add(name, node);
    }
}

impl VfsOps for FuseFS {
    fn mount(&self, _path: &str, mount_point: VfsNodeRef) -> VfsResult {
        info!("fusefs mount...");
        if let Some(parent) = mount_point.parent() {
            self.root.set_parent(Some(self.parent.call_once(|| parent)));
        } else {
            self.root.set_parent(None);
        }
        Ok(())
    }

    fn root_dir(&self) -> VfsNodeRef {
        info!("fusefs root_dir...");
        self.root.clone()
    }
}

impl Default for FuseFS {
    fn default() -> Self {
        info!("fusefs default...");
        Self::new()
    }
}

/// It implements [`axfs_vfs::VfsNodeOps`].
pub struct FuseNode {
    parent: RwLock<Weak<dyn VfsNodeOps>>,
    children: RwLock<BTreeMap<&'static str, VfsNodeRef>>,
    inode: SpinNoIrq<u64>,
    attr: SpinNoIrq<FuseAttr>,
    // name: SpinNoIrq<String>,
    size: SpinNoIrq<u64>, // file size
    flags: SpinNoIrq<u32>, // file flags
    fh: SpinNoIrq<u64>,
}

impl FuseNode {
    pub(super) fn new(parent: Option<&VfsNodeRef>, inode: u64, attr: FuseAttr, size: u64, fh: u64) -> Arc<Self> {
        info!("fuse_node new...");
        let parent = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
        Arc::new(Self {
            parent: RwLock::new(parent),
            children: RwLock::new(BTreeMap::new()),
            inode: SpinNoIrq::new(inode),
            attr: SpinNoIrq::new(attr),
            size: SpinNoIrq::new(size),
            flags: SpinNoIrq::new(0x8000),
            // name: SpinNoIrq::new(name),
            fh: SpinNoIrq::new(fh),
        })
    }

    pub(super) fn set_parent(&self, parent: Option<&VfsNodeRef>) {
        info!("fuse_node set_parent...");
        *self.parent.write() = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
    }

    pub fn is_dir(&self) -> bool {
        let attr_guard = self.attr.lock();
        let attr = &*attr_guard;
        let mode = attr.get_mode();
        // 0x8124 => file ( can read => 0x8000
        // 0x81a4 => file ( can read and write => 0x8001
        // 0x41ed => directory ( can read and execute
        // S_IFDIR = 0x4000
        // S_IFREG = 0x8000
        mode & 0x4000 == 0x4000
    }

    pub fn file_flags(&self) -> u32 {
        let attr_guard = self.attr.lock();
        let attr = &*attr_guard;
        let mode = attr.get_mode();
        let flags = match mode & 0x1c0 {
            0x80 => 0x8001, 
            0x100 => 0x8000, // 0x8124
            0x180 => 0x8001, // 0x81a4
            0x1c0 => 0x8002, // 0x81ed?
            _ => 0x8000,
        };
        flags
    }

    pub fn dir_flags(&self) -> u32 {
        let attr_guard = self.attr.lock();
        let attr = &*attr_guard;
        let mode = attr.get_mode();
        let flags = match mode & 0x1c0 {
            0x20 => 0x18801, // O_WRONLY
            0x40 => 0x18800, // O_RDONLY
            0x80 => 0x18802, // O_RDWR
            _ => 0x18800,
        };
        flags
    }

    pub fn check_init(&self) {
        let f1 = INITFLAG.load(Ordering::SeqCst);
        if f1 == 1 {
            INITFLAG.store(0, Ordering::Relaxed);
            self.init();
        }
    }

    /// Add a node to this directory.
    pub fn add(&self, name: &'static str, node: VfsNodeRef) {
        info!("fuse_node add {:?} ...", name);
        self.children.write().insert(name, node);
    }

    // FuseInit = 26
    pub fn init(&self) {
        info!("\nNEW FUSE REQUEST:\n  fuse_node INIT({:?}) here...", FuseOpcode::FuseInit as u32);

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            debug!("pid = {:?}", pid);
            let fusein = FuseInHeader::new(104, FuseOpcode::FuseInit as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 104];
            fusein.write_to(&mut fusebuf);
            let initin = FuseInitIn::new(7, 38, 0x00020000, 0x33fffffb, 0, [0; 11]);
            initin.write_to(&mut fusebuf[40..]);
            fusein.print();
            initin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at init in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseInit as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at init is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 80];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to init: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();
            // init_flag: 0x40F039
            let initout = FuseInitOut::read_from(&outbuf[16..]);
            initout.print();

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node init finish successfully...");
    }

    // FuseOpendir = 27
    pub fn open_dir(&self) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node OPENDIR({:?}) here...", FuseOpcode::FuseOpendir as u32);

        let opendir_error;
        let mut opendirout = FuseOpenOut::default();

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(48, FuseOpcode::FuseOpendir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let openin = FuseOpenIn::new(0x18800, 0);
            openin.write_to(&mut fusebuf[40..]);
            fusein.print();
            openin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at open_dir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseOpendir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at open_dir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 32];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to open_dir: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                opendirout = FuseOpenOut::read_from(&outbuf[16..]);
                opendirout.print();
                opendir_error = 1;
            }
            else {
                opendir_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node open_dir finish successfully...");

        if opendir_error < 0 {
            match opendir_error {
                -13 => return Err(VfsError::PermissionDenied),
                -20 => return Err(VfsError::NotADirectory),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }
        
        let mut fh_guard = self.fh.lock();
        let fh = &mut *fh_guard;
        *fh = opendirout.get_fh();

        info!("fh = {:#x}", fh);
        info!("opendirout.fh = {:#x}", opendirout.get_fh());

        Ok(())
    }

    // FuseReleasedir = 28
    pub fn release_dir(&self) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RELEASEDIR({:?}) here...", FuseOpcode::FuseReleasedir as u32);

        let releasedir_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(64, FuseOpcode::FuseReleasedir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 64];
            fusein.write_to(&mut fusebuf);
            let releasein = FuseReleaseIn::new(fh, 0x18800, 0, 0);
            releasein.write_to(&mut fusebuf[40..]);
            fusein.print();
            releasein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at release_dir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseReleasedir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at release_dir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to release_dir: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                releasedir_error = 1;
            }
            else {
                releasedir_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node release_dir finish successfully...");

        if releasedir_error < 0 {
            match releasedir_error {
                -13 => return Err(VfsError::PermissionDenied),
                -20 => return Err(VfsError::NotADirectory),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        } else {
            Ok(())
        }
    }

    // FuseFlush = 25
    pub fn flush(&self) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node FLUSH({:?}) here...", FuseOpcode::FuseFlush as u32);

        let flush_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(64, FuseOpcode::FuseFlush as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 64];
            fusein.write_to(&mut fusebuf);
            let flushin = FuseFlushIn::new(fh, 0, 0, 0);
            flushin.write_to(&mut fusebuf[40..]);
            fusein.print();
            flushin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at flush in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseFlush as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at flush is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to flush: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                flush_error = 1;
            }
            else {
                flush_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node flush finish successfully...");

        if flush_error < 0 {
            match flush_error {
                -13 => return Err(VfsError::PermissionDenied),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        } else {
            Ok(())
        }
    }

    // FuseMknod = 8
    pub fn mknod(&self, name: &str) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node MKNOD({:?}) {:?} here...", FuseOpcode::FuseMknod as u32, name);

        let mknod_error;
        let mknodout;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let name_len = name.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(57 + name_len as u32, FuseOpcode::FuseMknod as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf);
            let mknodin = FuseMknodIn::new(0x81a4, 0, 0);
            mknodin.write_to(&mut fusebuf[40..]);
            fusebuf[56..56 + name_len].copy_from_slice(name.as_bytes());
            fusein.print();
            mknodin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at mknod in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseMknod as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at mknod is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 144];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to mknod: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);

            if fuseout.is_ok() {
                mknodout = FuseEntryOut::read_from(&outbuf[16..]);
                mknodout.print();
                mknod_error = 1;
            }
            else {
                mknod_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);

            info!("fuse_node mknod finish successfully...");

            if mknod_error < 0 {
                match mknod_error {
                    -13 => return Err(VfsError::PermissionDenied),
                    -17 => return Err(VfsError::AlreadyExists),
                    -38 => return Err(VfsError::FunctionNotImplemented),
                    _ => return Err(VfsError::PermissionDenied),
                }
            }

            Ok(())
        }
    }

    // FuseMkdir = 9
    pub fn mkdir(&self, name: &str) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node MKDIR({:?}) {:?} here...", FuseOpcode::FuseMkdir as u32, name);

        let mkdir_error;
        let mkdirout;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let name_len = name.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(49 + name_len as u32, FuseOpcode::FuseMkdir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf);
            let mkdirin = FuseMkdirIn::new(755, 22);
            mkdirin.write_to(&mut fusebuf[40..]);
            fusebuf[48..48 + name_len].copy_from_slice(name.as_bytes());
            fusein.print();
            mkdirin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at mkdir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseMkdir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at mkdir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 144];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to mkdir: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                mkdirout = FuseEntryOut::read_from(&outbuf[16..]);
                mkdirout.print();
                mkdir_error = 1;
            }
            else {
                mkdir_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node mkdir finish successfully...");        

        if mkdir_error < 0 {
            match mkdir_error {
                -13 => return Err(VfsError::PermissionDenied),
                -17 => return Err(VfsError::AlreadyExists),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }
        
        Ok(())
    }

    // FuseRmdir = 11
    pub fn rmdir(&self, name: &str) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RMDIR({:?}) {:?} here...", FuseOpcode::FuseRmdir as u32, name);

        let rmdir_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let name_len = name.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(41 + name_len as u32, FuseOpcode::FuseRmdir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf);
            fusebuf[40..40 + name_len].copy_from_slice(name.as_bytes());
            fusein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at rmdir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRmdir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at rmdir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to rmdir: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                rmdir_error = 1;
            }
            else {
                rmdir_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);

            info!("fuse_node rmdir finish successfully...");

            if rmdir_error < 0 {
                match rmdir_error {
                    -2 => return Err(VfsError::NotFound),
                    -13 => return Err(VfsError::PermissionDenied),
                    -20 => return Err(VfsError::NotADirectory),
                    -38 => return Err(VfsError::FunctionNotImplemented),
                    _ => return Err(VfsError::PermissionDenied),
                }
            }

            Ok(())

        }
    }

    // FuseRename2 = 45
    pub fn rename2(&self, old: &str, new: &str) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RENAME2({:?}) from {:?} to {:?} here...", FuseOpcode::FuseRename2 as u32, old, new);

        let rename_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let old_len = old.len();
            let new_len = new.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(58 + (old_len + new_len) as u32, FuseOpcode::FuseRename2 as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 280];
            fusein.write_to(&mut fusebuf);
            let rename2in = FuseRename2In::new(1, 1);
            rename2in.write_to(&mut fusebuf[40..]);
            fusebuf[56..56 + old_len].copy_from_slice(old.as_bytes());
            fusebuf[57 + old_len..57 + old_len + new_len].copy_from_slice(new.as_bytes());
            fusein.print();
            rename2in.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at rename2 in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRename2 as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at rename2 is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to rename2: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                rename_error = 1;
            }
            else {
                rename_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);

            info!("fuse_node rename2 from {:?} to {:?} finish successfully...", old, new);

            if rename_error < 0 {
                match rename_error {
                    -2 => return Err(VfsError::NotFound),
                    -13 => return Err(VfsError::PermissionDenied),
                    -38 => return Err(VfsError::FunctionNotImplemented),
                    _ => return Err(VfsError::PermissionDenied),
                }
            }

            Ok(())
        }
    }

    // FuseAccess = 34
    pub fn access(&self) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node ACCESS({:?}) here...", FuseOpcode::FuseAccess as u32);

        let access_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(48, FuseOpcode::FuseAccess as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let accessin = FuseAccessIn::new(1);
            accessin.write_to(&mut fusebuf[40..]);
            fusein.print();
            accessin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at access in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseAccess as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at access is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to access: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                access_error = 1;
            }
            else {
                access_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);

            info!("fuse_node access finish successfully...");

            if access_error < 0 {
                match access_error {
                    -13 => return Err(VfsError::PermissionDenied),
                    -38 => return Err(VfsError::FunctionNotImplemented),
                    _ => return Err(VfsError::PermissionDenied),
                }
            }

            Ok(())
        }
    }

}

impl VfsNodeOps for FuseNode {
    // FuseOpen = 14
    fn open(&self) -> VfsResult {
        if self.is_dir() {
            return self.open_dir()
        }
        
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node OPEN({:?}) here...", FuseOpcode::FuseOpen as u32);

        let open_error;
        let mut openout = FuseOpenOut::default();

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            
            let fusein = FuseInHeader::new(48, FuseOpcode::FuseOpen as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let mut flags = self.file_flags();
            flags = 0x8000;
            // if flags == 0x8001 {
            //     flags = 0x8201;
            // }

            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}, flags: {:#x}", pid, nodeid, fh, size, self.is_dir(), flags);
            let openin = FuseOpenIn::new(flags, 0);
            openin.write_to(&mut fusebuf[40..]);
            fusein.print();
            openin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at open in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseOpen as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at open is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 32];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to open: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();
            if fuseout.is_ok() {
                openout = FuseOpenOut::read_from(&outbuf[16..]);
                openout.print();
                open_error = 1;
            }
            else {
                open_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node open finish successfully...");

        if open_error < 0 {
            match open_error {
                -13 => return Err(VfsError::PermissionDenied),
                -21 => return Err(VfsError::IsADirectory),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        let mut fh_guard = self.fh.lock();
        let fh = &mut *fh_guard;
        *fh = openout.get_fh();

        info!("fh = {:#x}", fh);
        info!("openout.fh = {:#x}", openout.get_fh());
        
        Ok(())
    }

    // FuseRelease = 18
    fn release(&self) -> VfsResult {
        if self.is_dir() {
            return self.release_dir()            
        }

        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RELEASE({:?}) here...", FuseOpcode::FuseRelease as u32);

        let release_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;

            let fusein = FuseInHeader::new(64, FuseOpcode::FuseRelease as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 64];
            fusein.write_to(&mut fusebuf);
            let flags_guard = self.flags.lock();
            let flags = *flags_guard;


            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}, flags: {:#x}", pid, nodeid, fh, size, self.is_dir(), flags);
            let releasein = FuseReleaseIn::new(fh, flags, 0, 0);
            releasein.write_to(&mut fusebuf[40..]);
            fusein.print();
            releasein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at release in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRelease as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at release is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to release: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                release_error = 1;
            }
            else {
                release_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node release finish successfully...");
        
        if release_error < 0 {
            match release_error {
                -13 => return Err(VfsError::PermissionDenied),
                -21 => return Err(VfsError::IsADirectory),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        } else {
            Ok(())
        }
    }

    // FuseGetattr = 3
    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node GET_ATTR({:?}) here...", FuseOpcode::FuseGetattr as u32);
    
        let attr_size;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(56, FuseOpcode::FuseGetattr as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 56];
            fusein.write_to(&mut fusebuf);
            let getattrin = FuseGetattrIn::new(0, 0, fh);
            getattrin.write_to(&mut fusebuf[40..]);
            fusein.print();
            getattrin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at get_attr in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseGetattr as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at get_attr is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 120];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to get_attr: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();
            let fuseattr = FuseAttrOut::read_from(&outbuf[16..]);
            fuseattr.print();

            let mut attr_guard = self.attr.lock();
            let attr = &mut *attr_guard;
            *attr = fuseattr.get_attr();
            let mut size_guard = self.size.lock();
            let size = &mut *size_guard;
            *size = fuseattr.get_size();
            attr_size = fuseattr.get_size();

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node get_attr finish successfully...");

        if self.is_dir() {
            Ok(VfsNodeAttr::new_dir(attr_size, 0))
        } else {
            Ok(VfsNodeAttr::new_file(attr_size, 0))
        }

    }

    fn parent(&self) -> Option<VfsNodeRef> {
        self.parent.read().upgrade()
    }

    // FuseRead = 15
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node READ({:?}) here, offset: {:?}, buf_len: {:?}...", FuseOpcode::FuseRead as u32, offset, buf.len());

        let read_error;
        let mut outlen = 0;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;

            let fusein = FuseInHeader::new(80, FuseOpcode::FuseRead as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 80];
            fusein.write_to(&mut fusebuf);

            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());
            let mut flags_guard = self.flags.lock();
            let readflags = &mut *flags_guard;
            *readflags = 0x8000;
            let readin = FuseReadIn::new(fh, offset, 4096, 0, 0, 0x8000);
            readin.write_to(&mut fusebuf[40..]);
            fusein.print();
            readin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at read in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRead as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at read is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 4096+16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to read: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                outlen = vec.len() - 16;
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                let readout = &outbuf[16..outlen+16];
                buf[..outlen].copy_from_slice(readout);
                info!("readout: {:?}", readout);
                read_error = 1;
            }
            else {
                read_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node read finish successfully...");



        if read_error < 0 {
            match read_error {
                -13 => return Err(VfsError::PermissionDenied),
                -21 => return Err(VfsError::IsADirectory),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        } else {
            Ok(outlen)
        }
    }

    // FuseWrite = 16
    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node WRITE({:?}) here, offset: {:?}, buf_len: {:?}, buf: {:?}...", FuseOpcode::FuseWrite as u32, offset, buf.len(), buf);

        let write_error;
        let writeout;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let buf_len = buf.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;

            let fusein = FuseInHeader::new(81 + buf_len as u32, FuseOpcode::FuseWrite as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 12000];
            fusein.write_to(&mut fusebuf);
            let flags = self.file_flags();
            let mut flags_guard = self.flags.lock();
            let wflags = &mut *flags_guard;
            *wflags = flags;

            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}, flags: {:#x}", pid, nodeid, fh, size, self.is_dir(), flags);
            let writein = FuseWriteIn::new(fh, offset, (buf_len+1) as u32, 0, 0, flags);
            writein.write_to(&mut fusebuf[40..]);
            fusebuf[80..80 + buf_len].copy_from_slice(buf);
            fusein.print();
            writein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at write in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseWrite as i32, Ordering::Relaxed);
            
            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at write is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 24];
            
            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to write: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();
            
            if fuseout.is_ok() {
                writeout = FuseWriteOut::read_from(&outbuf[16..]);
                writeout.print();
                write_error = 1;
            }
            else {
                write_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node write finish successfully...");

        if write_error < 0 {
            match write_error {
                -2 => return Err(VfsError::NotFound),
                -13 => return Err(VfsError::PermissionDenied),
                -21 => return Err(VfsError::IsADirectory),
                // -30 => return Err(VfsError::NoSpace),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        } else {
            Ok(buf.len())
        }
    }

    // FuseFsync = 20
    fn fsync(&self) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node FSYNC({:?}) here...", FuseOpcode::FuseFsync as u32);

        let fsync_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(40, FuseOpcode::FuseFsync as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);
            fusein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at fsync in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseFsync as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at fsync is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to fsync: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                fsync_error = 1;
            }
            else {
                fsync_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node fsync finish successfully...");

        if fsync_error < 0 {
            match fsync_error {
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        } else {
            Ok(())
        }
    }

    // FuseLookup = 1
    fn lookup(self: Arc<Self>, raw_path: &str) -> VfsResult<VfsNodeRef> {
        self.check_init();
        let path = raw_path.trim_start_matches('/');
        info!("\nNEW FUSE REQUEST:\n  fuse_node LOOKUP({:?}) {:?} here...", FuseOpcode::FuseLookup as u32, path);

        let lookup_error;
        let mut entryout = FuseEntryOut::default();

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let path_len = path.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(41 + path_len as u32, FuseOpcode::FuseLookup as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf[0..40]);
            fusebuf[40..40+path_len].copy_from_slice(path.as_bytes());
            fusein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at lookup in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseLookup as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at lookup is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 144];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to lookup: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf[..16]);
            fuseout.print();

            if fuseout.is_ok() {
                entryout = FuseEntryOut::read_from(&outbuf[16..]);
                entryout.print();
                lookup_error = 1;
            }
            else {
                lookup_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node lookup finish successfully...");

        if lookup_error < 0 {
            match lookup_error {
                -2 => return Err(VfsError::NotFound),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        let (name, rest) = split_path(path);
        if let Some(rest) = rest {
            self.lookup(rest)
        } else {
            match name {
                "" | "." => {
                    let mut ino_guard = self.inode.lock();
                    let ino = &mut *ino_guard;
                    *ino = entryout.get_nodeid();
                    let mut attr_guard = self.attr.lock();
                    let attr = &mut *attr_guard;
                    *attr = entryout.get_attr();
                    let mut size_guard = self.size.lock();
                    let size = &mut *size_guard;
                    *size = entryout.get_size();
                    debug!("lookup entryout.inode is {:?}...", entryout.get_nodeid());
                    debug!("lookup modify inode to {:?}...", self.inode.lock());
                    return Ok(self.clone() as VfsNodeRef);
                },
                ".." => {
                    return self.parent().ok_or(VfsError::NotFound)
                }
                _ => {
                    let parent = self.clone() as VfsNodeRef;
                    let node = Self::new(Some(&parent), entryout.get_nodeid(), entryout.get_attr(), entryout.get_size(), 0);
                    // self.children.write().insert(path, node.clone());
                    return Ok(node)
                },
            };
        }

    }

    // FuseCreate = 20
    fn create(&self, path: &str, ty: VfsNodeType) -> VfsResult {
        if ty == VfsNodeType::Dir {
            return self.mkdir(path)
        }

        let (name, rest) = split_path(path);
        if let Some(rest) = rest {
            if name == "." {
                return self.create(rest, ty)
            }
            if name == ".." {
                return self.parent().ok_or(VfsError::NotFound)?.create(rest, ty)
            }
        }

        self.check_init();
        let newtype = match ty {
            VfsNodeType::Fifo => "fifo",
            VfsNodeType::CharDevice => "char device",
            VfsNodeType::BlockDevice => "block device",
            VfsNodeType::File => "file",
            VfsNodeType::SymLink => "symlink",
            VfsNodeType::Socket => "socket",
            _ => "unknown",
        };
        info!("\nNEW FUSE REQUEST:\n  fuse_node CREATE({:?}) {:?} here, type: {:?}...", FuseOpcode::FuseCreate as u32, path, newtype);

        let create_error;
        let createout;
        let openout;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let path_len = path.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(57 + path_len as u32, FuseOpcode::FuseCreate as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf);
            let createin = FuseCreateIn::new(0x8241, 100644, 22, 0);
            createin.write_to(&mut fusebuf[40..]);
            fusebuf[56..56+path_len].copy_from_slice(path.as_bytes());
            fusein.print();
            createin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at create in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseCreate as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at create is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 160];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to create: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                createout = FuseEntryOut::read_from(&outbuf[16..]);
                createout.print();
                openout = FuseOpenOut::read_from(&outbuf[144..]);
                openout.print();
                create_error = 1;
            }
            else {
                create_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node create finish successfully...");

        if create_error < 0 {
            match create_error {
                -13 => return Err(VfsError::PermissionDenied),
                -17 => return Err(VfsError::AlreadyExists),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        Ok(())
    }

    // FuseForget = 2
    fn remove(&self, path: &str) -> VfsResult {
        if self.is_dir() {
            return self.rmdir(path)
        }

        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node UNLINK({:?}) {:?} here...", FuseOpcode::FuseForget as u32, path);

        let remove_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let path_len = path.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(41 + path_len as u32, FuseOpcode::FuseForget as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf);
            fusebuf[40..40+path_len].copy_from_slice(path.as_bytes());
            fusein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at remove in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseForget as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at remove is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to remove: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                remove_error = 1;
            }
            else {
                remove_error = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node remove finish successfully...");

        if remove_error < 0 {
            match remove_error {
                -2 => return Err(VfsError::NotFound),
                -13 => return Err(VfsError::PermissionDenied),
                -21 => return Err(VfsError::IsADirectory),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        // let (name, rest) = split_path(path);
        // if let Some(rest) = rest {
        //     match name {
        //         "" | "." => self.remove(rest),
        //         ".." => self.parent().ok_or(VfsError::NotFound)?.remove(rest),
        //         _ => self
        //             .children
        //             .read()
        //             .get(name)
        //             .ok_or(VfsError::NotFound)?
        //             .remove(rest),
        //     }
        // } else {
        //     Err(VfsError::PermissionDenied)
        // }

        Ok(())
    }

    // FuseReaddir = 28
    fn read_dir(&self, start_idx: usize, dirents: &mut [VfsDirEntry]) -> VfsResult<usize> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node READ_DIR({:?}) here, start: {:?}...", FuseOpcode::FuseReaddir as u32, start_idx);

        let readdir_error;
        let mut dirs = Vec::<FuseDirent>::new();

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(80, FuseOpcode::FuseReaddir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 80];
            fusein.write_to(&mut fusebuf);
            let readin = FuseReadIn::new(fh, 0, 4096, 0, 0, 0x18800);
            readin.write_to(&mut fusebuf[40..]);
            fusein.print();
            readin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at readdir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseReaddir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at readdir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 12000];
            let mut buf_len = 0;

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to readdir: {:?}", vec);
                buf_len = vec.len();
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                readdir_error = 1;
            }
            else {
                readdir_error = fuseout.error();
            }

            let mut offset = 16;
            while offset < buf_len {
                let direntry = FuseDirent::read_from(&outbuf[offset..]);
                direntry.print();
                offset += direntry.get_len() as usize;
                dirs.push(direntry);
                info!("offset = {:?}", offset);
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node readdir finish successfully...");

        if readdir_error < 0 {
            match readdir_error {
                -13 => return Err(VfsError::PermissionDenied),
                -20 => return Err(VfsError::NotADirectory),
                -22 => return Err(VfsError::InvalidInput),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        for (i, ent) in dirents.iter_mut().enumerate() {
            match i + start_idx {
                0 => *ent = VfsDirEntry::new(".", VfsNodeType::Dir),
                1 => *ent = VfsDirEntry::new("..", VfsNodeType::Dir),
                _ => {
                    debug!("dirs.len() = {:?}, i+idx = {:?}", dirs.len(), i + start_idx);
                    if let Some(entry) = dirs.get(i + start_idx) {
                        let entry_type = entry.get_type_as_vfsnodetype();
                        *ent = VfsDirEntry::new(&entry.get_name(), entry_type);
                        debug!("entry: {{ name: {:?}, type: {:?} }}", &entry.get_name(), entry_type);
                    } else {
                        for j in 0..i {
                            info!("entry {:?}: name: {:?}, type: {:?}", j, String::from_utf8(dirents[j].name_as_bytes().to_vec()), &dirents[j].entry_type());
                        }
                        info!("Ok(i) = {:?}", i);
                        return Ok(i);
                    }
                }
            }
        }

        for j in 0..dirents.len() {
            info!("entry {:?}: name: {:?}, type: {:?}", j, String::from_utf8(dirents[j].name_as_bytes().to_vec()), &dirents[j].entry_type());
        }
        
        info!("Ok(dirents.len()) = {:?}", dirents.len());
        Ok(dirents.len())
    }

    // FuseRename = 12
    fn rename(&self, src_path: &str, dst_path: &str) -> VfsResult {
        // self.rename2(src_path, dst_path);

        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RENAME({:?}) from {:?} to {:?} here...", FuseOpcode::FuseRename as u32, src_path, dst_path);

        let rename_error;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            let src_len = src_path.len();
            let dst_len = dst_path.len();
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fh_guard = self.fh.lock();
            let fh = *fh_guard;
            let size_guard = self.size.lock();
            let size = *size_guard;
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}, is_dir: {:?}", pid, nodeid, fh, size, self.is_dir());

            let fusein = FuseInHeader::new(50 + (src_len + dst_len) as u32, FuseOpcode::FuseRename as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32);
            let mut fusebuf = [0; 280];
            fusein.write_to(&mut fusebuf);
            let renamein = FuseRenameIn::new(1);
            renamein.write_to(&mut fusebuf[40..]);
            fusebuf[48..48 + src_len].copy_from_slice(src_path.as_bytes());
            fusebuf[49 + src_len..49 + src_len + dst_len].copy_from_slice(dst_path.as_bytes());
            fusein.print();
            renamein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                debug!("Fusevec at rename in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRename as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    debug!("Fuseflag at rename is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                debug!("Fusevec back to rename: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            if fuseout.is_ok() {
                rename_error = 1;
            }
            else {
                rename_error = fuseout.error();
            }
            
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node rename from {:?} to {:?} finish successfully...", src_path, dst_path);

        if rename_error < 0 {
            match rename_error {
                -2 => return Err(VfsError::NotFound),
                -13 => return Err(VfsError::PermissionDenied),
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        Ok(())
    }

}

fn split_path(path: &str) -> (&str, Option<&str>) {
    let trimmed_path = path.trim_start_matches('/');
    trimmed_path.find('/').map_or((trimmed_path, None), |n| {
        (&trimmed_path[..n], Some(&trimmed_path[n + 1..]))
    })
}

pub fn fusefs() -> Arc<FuseFS> {
    debug!("fusefs newfs here...");
    Arc::new(FuseFS::new())
}