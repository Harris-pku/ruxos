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
    FuseAttr, FuseAttrOut, FuseCreateIn, FuseDirent, FuseEntryOut, FuseFlushIn, FuseGetattrIn, FuseInHeader, FuseInitIn, FuseInitOut, FuseOpcode, FuseOpenIn, FuseOpenOut, FuseOutHeader, FuseReadIn, FuseReleaseIn, FuseRenameIn, FuseWriteIn, FuseWriteOut
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

    /// Create a subdirectory at the root directory.
    pub fn mkdir(&self, name: &'static str) -> Arc<FuseNode> {
        info!("fusefs mkdir...");
        self.root.mkdir(name)
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
    size: SpinNoIrq<u64>, // file size, 0 if it's a directory
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
            // name: SpinNoIrq::new(name),
            fh: SpinNoIrq::new(fh),
        })
    }

    pub(super) fn set_parent(&self, parent: Option<&VfsNodeRef>) {
        info!("fuse_node set_parent...");
        // self.init();
        *self.parent.write() = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
    }

    pub fn check_init(&self) {
        let f1 = INITFLAG.load(Ordering::SeqCst);
        if f1 == 1 {
            INITFLAG.store(0, Ordering::Relaxed);
            self.init();
        }
    }

    /// Create a subdirectory at this directory.
    // FuseMkdir = 9
    pub fn mkdir(self: &Arc<Self>, name: &'static str) -> Arc<Self> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node MKDIR({:?}) {:?} here...", FuseOpcode::FuseMkdir as u32, name);

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);
            let nodeid_guard = self.inode.lock();
            let nodeid = *nodeid_guard;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseMkdir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at mkdir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseMkdir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at mkdir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 120];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to mkdir: {:?}", vec);
                outbuf.clone_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node mkdir finish successfully...");

        let parent = self.clone() as VfsNodeRef;
        let node = Self::new(Some(&parent), 0, FuseAttr::default(), 0, 0);
        self.children.write().insert(name, node.clone());
        node
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
            info!("pid = {:?}", pid);
            let fusein = FuseInHeader::new(104, FuseOpcode::FuseInit as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 104];
            fusein.write_to(&mut fusebuf);
            let initin = FuseInitIn::new(7, 38, 0x00020000, 0x33fffffb, 0, [0; 11]);
            initin.write_to(&mut fusebuf[40..]);
            fusein.print();
            initin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at init in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseInit as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at init is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 80];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to init: {:?}", vec);
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

            let fusein = FuseInHeader::new(48, FuseOpcode::FuseOpendir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let openin = FuseOpenIn::new(0, 0x18800);
            openin.write_to(&mut fusebuf[40..]);
            fusein.print();
            openin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at open_dir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseOpendir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at open_dir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 32];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to open_dir: {:?}", vec);
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(64, FuseOpcode::FuseReleasedir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 64];
            fusein.write_to(&mut fusebuf);
            let releasein = FuseReleaseIn::new(fh, 0x18800, 0, 0);
            releasein.write_to(&mut fusebuf[40..]);
            fusein.print();
            releasein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at release_dir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseReleasedir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at release_dir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to release_dir: {:?}", vec);
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(64, FuseOpcode::FuseFlush as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 64];
            fusein.write_to(&mut fusebuf);
            let flushin = FuseFlushIn::new(fh, 0, 0, 0);
            flushin.write_to(&mut fusebuf[40..]);
            fusein.print();
            flushin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at flush in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseFlush as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at flush is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to flush: {:?}", vec);
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
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        } else {
            Ok(())
        }
    }

}

impl VfsNodeOps for FuseNode {
    // FuseOpen = 14
    fn open(&self) -> VfsResult {
        let ssize_guard = self.size.lock();
        let ssize = *ssize_guard;
        if ssize == 0 {
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(48, FuseOpcode::FuseOpen as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let openin = FuseOpenIn::new(0, 0x18800);
            openin.write_to(&mut fusebuf[40..]);
            fusein.print();
            openin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at open in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseOpen as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at open is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 32];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to open: {:?}", vec);
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
        let ssize_guard = self.size.lock();
        let ssize = *ssize_guard;
        if ssize == 0 {
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(64, FuseOpcode::FuseRelease as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 64];
            fusein.write_to(&mut fusebuf);
            let releasein = FuseReleaseIn::new(fh, 0x18800, 0, 0);
            releasein.write_to(&mut fusebuf[40..]);
            fusein.print();
            releasein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at release in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRelease as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at release is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to release: {:?}", vec);
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
    
        let mut attr_size;

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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(56, FuseOpcode::FuseGetattr as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 56];
            fusein.write_to(&mut fusebuf);
            let getattrin = FuseGetattrIn::new(0, 0, fh);
            getattrin.write_to(&mut fusebuf[40..]);
            fusein.print();
            getattrin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at get_attr in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseGetattr as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at get_attr is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 120];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to get_attr: {:?}", vec);
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

        if attr_size > 0 {
            Ok(VfsNodeAttr::new_file(attr_size, 0))
        } else {
            Ok(VfsNodeAttr::new_dir(4096, 0))
        }
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(80, FuseOpcode::FuseRead as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 80];
            fusein.write_to(&mut fusebuf);
            let readin = FuseReadIn::new(fh, offset, 4096, 0, 0, 0x8000);
            readin.write_to(&mut fusebuf[40..]);
            fusein.print();
            readin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at read in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRead as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at read is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 4096+16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to read: {:?}", vec);
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
        info!("\nNEW FUSE REQUEST:\n  fuse_node WRITE({:?}) here, offset: {:?}, buf_len: {:?}...", FuseOpcode::FuseWrite as u32, offset, buf.len());

        let write_error;
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(120, FuseOpcode::FuseWrite as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 120];
            fusein.write_to(&mut fusebuf);
            let writein = FuseWriteIn::new(fh, offset, 0, 0, 0, 0x8000);
            writein.write_to(&mut fusebuf[40..]);
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
                    info!("Fuseflag at write is set to {:?}, exiting loop. !!!", flag);
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
            let writeout = FuseWriteOut::read_from(&outbuf[16..]);
            
            if fuseout.is_ok() {
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(40, FuseOpcode::FuseFsync as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);
            fusein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at fsync in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseFsync as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at fsync is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to fsync: {:?}", vec);
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
        let mut entryout ;

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
            info!("pid = {:?}, inode = {:?}, fh = {:#x} size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(41 + path_len as u32, FuseOpcode::FuseLookup as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf);
            fusebuf[40..40+path_len].copy_from_slice(path.as_bytes());
            fusein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at lookup in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseLookup as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at lookup is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 144];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to lookup: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf[..16]);
            fuseout.print();
            entryout = FuseEntryOut::read_from(&outbuf[16..]);

            if fuseout.is_ok() {
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
            let node = match name {
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
                    // let mut name_guard = self.name.lock();
                    // let mut name = *name_guard;
                    // name = String::from("");
                    // info!("lookup entryout.inode is {:?}...", entryout.get_nodeid());
                    // info!("lookup modify inode to {:?}...", self.inode.lock());
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
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node CREATE({:?}) {:?} here...", FuseOpcode::FuseCreate as u32, path);

        let create_error;

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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(56, FuseOpcode::FuseCreate as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 56];
            fusein.write_to(&mut fusebuf);
            let createin = FuseCreateIn::new(0, 0, 0, 0);
            createin.write_to(&mut fusebuf[40..]);
            fusein.print();
            createin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at create in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseCreate as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at create is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 160];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to create: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();
            // let entryout = FuseEntryOut::read_from(&outbuf[16..]);
            // entryout.print();
            if fuseout.is_ok() {
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
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        let (name, rest) = split_path(path);
        if let Some(rest) = rest {
            match name {
                "" | "." => self.create(rest, ty),
                ".." => self.parent().ok_or(VfsError::NotFound)?.create(rest, ty),
                _ => self
                    .children
                    .read()
                    .get(name)
                    .ok_or(VfsError::NotFound)?
                    .create(rest, ty),
            }
        } else if name.is_empty() || name == "." || name == ".." {
            Ok(())
        } else {
            Err(VfsError::PermissionDenied)
        }
    }

    // FuseForget = 2
    fn remove(&self, path: &str) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node REMOVE({:?}) {:?} here...", FuseOpcode::FuseForget as u32, path);

        let remove_error;

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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(40, FuseOpcode::FuseForget as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);
            fusein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at remove in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseForget as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at remove is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to remove: {:?}", vec);
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
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        let (name, rest) = split_path(path);
        if let Some(rest) = rest {
            match name {
                "" | "." => self.remove(rest),
                ".." => self.parent().ok_or(VfsError::NotFound)?.remove(rest),
                _ => self
                    .children
                    .read()
                    .get(name)
                    .ok_or(VfsError::NotFound)?
                    .remove(rest),
            }
        } else {
            Err(VfsError::PermissionDenied)
        }
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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(80, FuseOpcode::FuseReaddir as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 80];
            fusein.write_to(&mut fusebuf);
            let readin = FuseReadIn::new(fh, 0, 4096, 0, 0, 0x18800);
            readin.write_to(&mut fusebuf[40..]);
            fusein.print();
            readin.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at readdir in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseReaddir as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at readdir is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 1200];
            let mut buf_len = 0;

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to readdir: {:?}", vec);
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
                -38 => return Err(VfsError::FunctionNotImplemented),
                _ => return Err(VfsError::PermissionDenied),
            }
        }

        for (i, ent) in dirents.iter_mut().enumerate() {
            match i + start_idx {
                0 => *ent = VfsDirEntry::new(".", VfsNodeType::Dir),
                1 => *ent = VfsDirEntry::new("..", VfsNodeType::Dir),
                _ => {
                    info!("dirs.len() = {:?}, i+idx = {:?}", dirs.len(), i + start_idx);
                    if let Some(entry) = dirs.get(i + start_idx) {
                        *ent = VfsDirEntry::new(&entry.get_name(), VfsNodeType::File);
                        info!("entry = {:?}", &entry.get_name());
                    } else {
                        for j in 0..i {
                            info!("entry {:?}: name: {:?}, type: {:?}", j, String::from_utf8(dirents[j].name_as_bytes().to_vec()), &dirents[j].entry_type());
                        }
                        return Ok(i);
                    }
                }
            }
        }
        
        Ok(dirents.len())
    }

    // FuseRename = 12
    fn rename(&self, src_path: &str, dst_path: &str) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RENAME({:?}) from {:?} to {:?} here...", FuseOpcode::FuseRename as u32, src_path, dst_path);

        let rename_error;

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
            info!("pid = {:?}, inode = {:?}, fh = {:#x}, size = {:?}", pid, nodeid, fh, size);

            let fusein = FuseInHeader::new(48, FuseOpcode::FuseRename as u32, UNIQUE_ID, nodeid, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let renamein = FuseRenameIn::new(0);
            renamein.write_to(&mut fusebuf[40..]);
            fusein.print();
            renamein.print();

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&fusebuf);
                info!("Fusevec at rename in devfuse: {:?}", vec);
            }

            FUSEFLAG.store(FuseOpcode::FuseRename as i32, Ordering::Relaxed);

            loop {
                let flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag < 0 {
                    info!("Fuseflag at rename is set to {:?}, exiting loop. !!!", flag);
                    break;
                }
                ruxtask::yield_now();
            }

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to rename: {:?}", vec);
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