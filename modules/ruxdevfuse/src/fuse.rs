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
use spinlock::SpinNoIrq;
use core::sync::atomic::{AtomicI32, Ordering};
use alloc::vec::Vec;
use log::*;

use axfs_vfs::{VfsDirEntry, VfsError, VfsResult};
use axfs_vfs::{VfsNodeAttr, VfsNodeOps, VfsNodeRef, VfsNodeType, VfsOps};
use spin::{once::Once, RwLock};
use ruxfs::fuse_st::{FuseInHeader, FuseInitIn, FuseOpcode};
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
            root: FuseNode::new(None),
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
}

impl FuseNode {
    pub(super) fn new(parent: Option<&VfsNodeRef>) -> Arc<Self> {
        info!("fuse_node new...");
        let parent = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
        Arc::new(Self {
            parent: RwLock::new(parent),
            children: RwLock::new(BTreeMap::new()),
        })
    }

    pub(super) fn set_parent(&self, parent: Option<&VfsNodeRef>) {
        info!("fuse_node set_parent...");
        // self.init();
        *self.parent.write() = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
    }

    /// Create a subdirectory at this directory.
    pub fn mkdir(self: &Arc<Self>, name: &'static str) -> Arc<Self> {
        info!("\nNEW FUSE REQUEST:\n  fuse_node MKDIR here...");

        unsafe {
            // FuseMkdir = 9
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(104, FuseOpcode::FuseMkdir as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 104];
            fusein.write_to(&mut fusebuf);
            let initin = FuseInitIn::new(10, 229, 0, 0, 0, [0; 11]);
            initin.write_to(&mut fusebuf[40..]);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to mkdir: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node mkdir finish successfully...");

        let parent = self.clone() as VfsNodeRef;
        let node = Self::new(Some(&parent));
        self.children.write().insert(name, node.clone());
        node
    }

    /// Add a node to this directory.
    pub fn add(&self, name: &'static str, node: VfsNodeRef) {
        info!("fuse_node add...");
        self.children.write().insert(name, node);
    }

    pub fn init(&self) {
        info!("\nNEW FUSE REQUEST:\n  fuse_node INIT here...");

        unsafe {
            // FuseInit = 26
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(104, FuseOpcode::FuseInit as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 104];
            fusein.write_to(&mut fusebuf);
            let initin = FuseInitIn::new(10, 229, 0, 0, 0, [0; 11]);
            initin.write_to(&mut fusebuf[40..]);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to init: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node init finish successfully...");
    }
}

impl VfsNodeOps for FuseNode {
    fn open(&self) -> VfsResult {
        info!("\nNEW FUSE REQUEST:\n  fuse_node OPEN here...");

        unsafe {
            // FuseOpen = 14
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseOpen as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to open: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node open finish successfully...");
        
        Ok(())
    }

    fn release(&self) -> VfsResult {
        info!("\nNEW FUSE REQUEST:\n  fuse_node RELEASE here...");

        unsafe {
            // FuseRelease = 1
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseRelease as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to release: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node release finish successfully...");
        
        Ok(())
    }

    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        info!("\nNEW FUSE REQUEST:\n  fuse_node GET_ATTR here...");

        unsafe {
            // FuseGetattr = 3
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseGetattr as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to get_attr: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node get_attr finish successfully...");
        
        Ok(VfsNodeAttr::new_dir(4096, 0))
    }

    // fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    //     info!("fuse_node read_at here...");
    //     // FuseRead = 15
    //     FUSEFLAG.store(FuseOpcode::FuseRead as u32, Ordering::Relaxed);
        // let guard = self.children.read();
        // let mut data = guard.iter();
        // let mut offset = offset as usize;
        // let mut len = buf.len();
        // let mut i = 0;
        // while let Some((_, node)) = data.next() {
        //     let attr = node.get_attr().unwrap();
        //     let name = node.get_attr().unwrap().file_type();
        //     let size = attr.size();
        //     let file_type = attr.file_type();
        //     let perm = attr.perm();
        //     let uid = 0; // attr.uid();
        //     let gid = 0; // attr.gid();
        //     let atime = 0; // attr.atime();
        //     let mtime = 0; // attr.mtime();
        //     let ctime = 0; // attr.ctime();
        //     let nlink = 0; // attr.nlink();
        //     let rdev = 0; // attr.rdev();
        //     let blksize = 0; // attr.blksize();
        //     let blocks = 0; // attr.blocks();
        //     let flags = 0; // attr.flags();
        //     let gen = 0; // attr.gen();
        //     let birthtime = 0; // attr.birthtime();
        //     let attr = 0; // VfsNodeAttr::new(name, file_type, size, perm, uid, gid, atime, mtime, ctime, nlink, rdev, blksize, blocks, flags, gen, birthtime);
        //     let mut buf = [0; 1052672];
        //     let fusein = FuseInHeader::new(1052672, 15, 370, 2, 0, 0, 0, 0);
        //     fusein.write_to(&mut buf);
        //     let initin = FuseInitIn::new(10, 229, 0, 0, 0, [0; 11]);
        //     initin.write_to(&mut buf[40..]);
        //     let mut flag = 0;
        //     unsafe {
        //         if FUSE_VEC.is_none() {
        //             FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
        //         }
        //     }
        //     loop {
        //         flag = FUSEFLAG.load(Ordering::SeqCst);
        //         if flag != 0 {
        //             break;
        //         }
        //     }
        //     info!("Fuseflag is set to {:?}, exiting loop.", flag);
        //     FUSEFLAG.store(0, Ordering::Relaxed);
        //     unsafe {
        //         if let Some(vec_arc) = FUSE_VEC.as_ref() {
        //             let vec = vec_arc.lock();
        //             let len = vec.len();
        //             info!("Fusevec: {:?}", vec);
        //         }
        //     }
        //     i += 1;
        //     if i == len {
        //         break;
        //     }
        // }
    //     Ok(0)
    // }

    // fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
    //     info!("fuse_node write_at here...");
    //     // FuseWrite = 16
    //     FUSEFLAG.store(FuseOpcode::FuseWrite as u32, Ordering::Relaxed);
    //     Ok(0)
    // }

    // fn fsync(&self) -> VfsResult {
    //     info!("fuse_node fsync here...");
    //     // FuseFsync = 20
    //     FUSEFLAG.store(FuseOpcode::FuseFsync as u32, Ordering::Relaxed);
    //     Ok(())
    // }

    fn parent(&self) -> Option<VfsNodeRef> {
        info!("fuse_node parent here...");
        self.parent.read().upgrade()
    }

    fn lookup(self: Arc<Self>, path: &str) -> VfsResult<VfsNodeRef> {
        let f1 = INITFLAG.load(Ordering::SeqCst);
        if f1 == 1 {
            INITFLAG.store(0, Ordering::Relaxed);
            self.init();
        }

        info!("\nNEW FUSE REQUEST:\n  fuse_node LOOKUP here...");

        unsafe {
            // FuseLookup = 1
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseLookup as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to lookup: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node lookup finish successfully...");


        // let fusein = FuseInHeader::new(1052672, 1, 370, 2, 0, 0, 0, 0);
        // fusein.write_to(&mut [0; 1052672]);
        let (name, rest) = split_path(path);
        let node = match name {
            "" | "." => Ok(self.clone() as VfsNodeRef),
            ".." => self.parent().ok_or(VfsError::NotFound),
            _ => self
                .children
                .read()
                .get(name)
                .cloned()
                .ok_or(VfsError::NotFound),
        }?;

        if let Some(rest) = rest {
            node.lookup(rest)
        } else {
            Ok(node)
        }
    }

    fn create(&self, path: &str, ty: VfsNodeType) -> VfsResult {
        info!("\nNEW FUSE REQUEST:\n  fuse_node CREATE here...");

        unsafe {
            // FuseCreate = 20
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseCreate as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to create: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node create finish successfully...");
        

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

    fn remove(&self, path: &str) -> VfsResult {
        info!("\nNEW FUSE REQUEST:\n  fuse_node REMOVE here...");

        unsafe {
            // FuseForget = 2
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseForget as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to remove: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node remove finish successfully...");

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

    fn read_dir(&self, start_idx: usize, dirents: &mut [VfsDirEntry]) -> VfsResult<usize> {
        info!("\nNEW FUSE REQUEST:\n  fuse_node READ_DIR here...");

        unsafe {
            // FuseReaddir = 28
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseReaddir as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to readdir: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node readdir finish successfully...");
        
        let children = self.children.read();
        let mut children = children.iter().skip(start_idx.max(2) - 2);
        for (i, ent) in dirents.iter_mut().enumerate() {
            match i + start_idx {
                0 => *ent = VfsDirEntry::new(".", VfsNodeType::Dir),
                1 => *ent = VfsDirEntry::new("..", VfsNodeType::Dir),
                _ => {
                    if let Some((name, node)) = children.next() {
                        *ent = VfsDirEntry::new(name, node.get_attr().unwrap().file_type());
                    } else {
                        return Ok(i);
                    }
                }
            }
        }
        Ok(dirents.len())
    }

    fn rename(&self, _src_path: &str, _dst_path: &str) -> VfsResult {
        info!("\nNEW FUSE REQUEST:\n  fuse_node RENAME here...");

        unsafe {
            // FuseRename = 18
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 1;
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseRename as u32, UNIQUE_ID, 2, 0, 0, 0, 0);
            let mut fusebuf = [0; 40];
            fusein.write_to(&mut fusebuf);

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

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to rename: {:?}", vec);
                vec.clear();
            }
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node rename finish successfully...");

        Ok(())
    }

    axfs_vfs::impl_vfs_dir_default! {}
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