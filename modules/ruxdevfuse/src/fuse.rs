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
use ruxtask::current;
use spinlock::SpinNoIrq;
use core::sync::atomic::{AtomicI32, Ordering};
use alloc::vec::Vec;
use log::*;

use axfs_vfs::{VfsDirEntry, VfsError, VfsResult};
use axfs_vfs::{VfsNodeAttr, VfsNodeOps, VfsNodeRef, VfsNodeType, VfsOps};
use spin::{once::Once, RwLock};
use ruxfs::fuse_st::{
    FuseAttr, FuseAttrOut, FuseCreateIn, FuseEntryOut, FuseGetattrIn, FuseInHeader, FuseInitIn, FuseInitOut, FuseOpcode, FuseOpenIn, FuseOpenOut, FuseOutHeader, FuseReadIn, FuseReleaseIn, FuseRenameIn, FuseWriteIn, FuseWriteOut
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
        info!("\nNEW FUSE REQUEST:\n  fuse_node MKDIR {:?} here...", name);

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);            
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseMkdir as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
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
        let node = Self::new(Some(&parent));
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
        info!("\nNEW FUSE REQUEST:\n  fuse_node INIT here...");

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
        info!("fuse_node open_dir here...");
        Ok(())
    }
}

impl VfsNodeOps for FuseNode {
    // FuseOpen = 14
    fn open(&self) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node OPEN here...");

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);
            let fusein = FuseInHeader::new(48, FuseOpcode::FuseOpendir as u32, UNIQUE_ID, 2, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let openin = FuseOpenIn::new(0, 0x18800);
            openin.write_to(&mut fusebuf[40..]);
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
            let openout = FuseOpenOut::read_from(&outbuf[16..]);
            openout.print();

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node open finish successfully...");
        
        Ok(())
    }

    // FuseRelease = 18
    fn release(&self) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RELEASEDIR here...");

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);            
            let fusein = FuseInHeader::new(64, FuseOpcode::FuseReleasedir as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 64];
            fusein.write_to(&mut fusebuf);
            let releasein = FuseReleaseIn::new(18446603336224349984, 0x18800, 0, 0);
            releasein.write_to(&mut fusebuf[40..]);
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

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node release finish successfully...");
        
        Ok(())
    }

    // FuseGetattr = 3
    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node GET_ATTR here...");

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);                        
            let fusein = FuseInHeader::new(56, FuseOpcode::FuseGetattr as u32, UNIQUE_ID, 2, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 56];
            fusein.write_to(&mut fusebuf);
            let getattrin = FuseGetattrIn::new(0, 0, 0);
            getattrin.write_to(&mut fusebuf[40..]);
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

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node get_attr finish successfully...");
        
        Ok(VfsNodeAttr::new_dir(4096, 0))
    }

    // FuseRead = 15
    // fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {       
    //     self.check_init();
    //     info!("\nNEW FUSE REQUEST:\n  fuse_node READ here...");
    //     unsafe {
    //         if FUSE_VEC.is_none() {
    //             FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
    //         }
    //         UNIQUE_ID += 2;
    //         let pid = current().id().as_u64();
    //         info!("pid = {:?}", pid);            
    //         let fusein = FuseInHeader::new(80, FuseOpcode::FuseRead as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
    //         let mut fusebuf = [0; 80];
    //         fusein.write_to(&mut fusebuf);
    //         let openin = FuseReadIn::new(0, 0, 0, 0, 0, 0);
    //         openin.write_to(&mut fusebuf[40..]);
    //         if let Some(vec_arc) = FUSE_VEC.as_ref() {
    //             let mut vec = vec_arc.lock();
    //             vec.extend_from_slice(&fusebuf);
    //             info!("Fusevec at read in devfuse: {:?}", vec);
    //         }
    //         FUSEFLAG.store(FuseOpcode::FuseRead as i32, Ordering::Relaxed);
    //         loop {
    //             let flag = FUSEFLAG.load(Ordering::SeqCst);
    //             if flag < 0 {
    //                 info!("Fuseflag at read is set to {:?}, exiting loop. !!!", flag);
    //                 break;
    //             }
    //             ruxtask::yield_now();
    //         }
    //         let mut outbuf = [0; 16];
    //         if let Some(vec_arc) = FUSE_VEC.as_ref() {
    //             let mut vec = vec_arc.lock();
    //             info!("Fusevec back to read: {:?}", vec);
    //             outbuf[0..vec.len()].copy_from_slice(&vec);
    //             vec.clear();
    //         }
    //         let fuseout = FuseOutHeader::read_from(&outbuf);
    //         fuseout.print();
    //         FUSEFLAG.store(0, Ordering::Relaxed);
    //     }
    //     info!("fuse_node read finish successfully...");
    //     Ok(0)
    // }

    // FuseWrite = 16
    // fn write_at(&self, offset: u64, _buf: &[u8]) -> VfsResult<usize> {
    //     self.check_init();
    //     info!("\nNEW FUSE REQUEST:\n  fuse_node WRITE here...");
    //     unsafe {
    //         if FUSE_VEC.is_none() {
    //             FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
    //         }
    //         UNIQUE_ID += 2;
    //         let pid = current().id().as_u64();
    //         info!("pid = {:?}", pid);            
    //         let fusein = FuseInHeader::new(120, FuseOpcode::FuseWrite as u32, UNIQUE_ID, 2, 1000, 1000, pid as u32, 0);
    //         let mut fusebuf = [0; 120];
    //         fusein.write_to(&mut fusebuf);
    //         let writein = FuseWriteIn::new(0, 0, 0, 0, 0, 0);
    //         writein.write_to(&mut fusebuf[40..]);
    //         if let Some(vec_arc) = FUSE_VEC.as_ref() {
    //             let mut vec = vec_arc.lock();
    //             vec.extend_from_slice(&fusebuf);
    //             info!("Fusevec at write in devfuse: {:?}", vec);
    //         }
    //         FUSEFLAG.store(FuseOpcode::FuseWrite as i32, Ordering::Relaxed);
    //         loop {
    //             let flag = FUSEFLAG.load(Ordering::SeqCst);
    //             if flag < 0 {
    //                 info!("Fuseflag at write is set to {:?}, exiting loop. !!!", flag);
    //                 break;
    //             }
    //             ruxtask::yield_now();
    //         }
    //         let mut outbuf = [0; 24];
    //         if let Some(vec_arc) = FUSE_VEC.as_ref() {
    //             let mut vec = vec_arc.lock();
    //             info!("Fusevec back to write: {:?}", vec);
    //             outbuf[0..vec.len()].copy_from_slice(&vec);
    //             vec.clear();
    //         }
    //         let fuseout = FuseOutHeader::read_from(&outbuf);
    //         fuseout.print();
    //         let writeout = FuseWriteOut::read_from(&outbuf[16..]);
    //         writeout.print();
    //         FUSEFLAG.store(0, Ordering::Relaxed);
    //     }
    //     info!("fuse_node write finish successfully...");
    //     Ok(0)
    // }

    // FuseFsync = 20
    // fn fsync(&self) -> VfsResult {
    //     self.check_init();
    //     info!("\nNEW FUSE REQUEST:\n  fuse_node FSYNC here...");
    //     unsafe {
    //         if FUSE_VEC.is_none() {
    //             FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
    //         }
    //         UNIQUE_ID += 2;
    //         let pid = current().id().as_u64();
    //         info!("pid = {:?}", pid);            
    //         let fusein = FuseInHeader::new(40, FuseOpcode::FuseFsync as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
    //         let mut fusebuf = [0; 40];
    //         fusein.write_to(&mut fusebuf);
    //         if let Some(vec_arc) = FUSE_VEC.as_ref() {
    //             let mut vec = vec_arc.lock();
    //             vec.extend_from_slice(&fusebuf);
    //             info!("Fusevec at fsync in devfuse: {:?}", vec);
    //         }
    //         FUSEFLAG.store(FuseOpcode::FuseFsync as i32, Ordering::Relaxed);
    //         loop {
    //             let flag = FUSEFLAG.load(Ordering::SeqCst);
    //             if flag < 0 {
    //                 info!("Fuseflag at fsync is set to {:?}, exiting loop. !!!", flag);
    //                 break;
    //             }
    //             ruxtask::yield_now();
    //         }
    //         if let Some(vec_arc) = FUSE_VEC.as_ref() {
    //             let mut vec = vec_arc.lock();
    //             info!("Fusevec back to fsync: {:?}", vec);
    //             vec.clear();
    //         }
    //         FUSEFLAG.store(0, Ordering::Relaxed);
    //     }
    //     info!("fuse_node fsync finish successfully...");
    //     Ok(())
    // }

    // FuseLookup = 1
    fn lookup(self: Arc<Self>, path: &str) -> VfsResult<VfsNodeRef> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node LOOKUP {:?} here...", path);

        let mut _lookup_flag = 0;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);
            let path_len = path.len();
            let fusein = FuseInHeader::new(41 + path_len as u32, FuseOpcode::FuseLookup as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 180];
            fusein.write_to(&mut fusebuf);
            fusebuf[40..40+path_len].copy_from_slice(path.as_bytes());

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
            let entryout = FuseEntryOut::read_from(&outbuf[16..]);
            if fuseout.is_ok() {
                entryout.print();
                _lookup_flag = 1;
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node lookup finish successfully...");

        // if lookup_flag == 0 {
        //     return Err(VfsError::NotFound);
        // }

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

    // FuseCreate = 20
    fn create(&self, path: &str, ty: VfsNodeType) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node CREATE {:?} here...", path);

        let mut create_flag = 0;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);
            let fusein = FuseInHeader::new(56, FuseOpcode::FuseCreate as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 56];
            fusein.write_to(&mut fusebuf);
            let createin = FuseCreateIn::new(0, 0, 0, 0);
            createin.write_to(&mut fusebuf[40..]);
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
                create_flag = 1;
            }
            else {
                create_flag = fuseout.error();
            }

            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node create finish successfully...");

        if create_flag < 0 {
            match create_flag {
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
        info!("\nNEW FUSE REQUEST:\n  fuse_node REMOVE {:?} here...", path);

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);
            let fusein = FuseInHeader::new(40, FuseOpcode::FuseForget as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
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

            let mut outbuf = [0; 16];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to remove: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

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

    // FuseReaddir = 28
    fn read_dir(&self, start_idx: usize, dirents: &mut [VfsDirEntry]) -> VfsResult<usize> {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node READ_DIR here...");

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);
            let fusein = FuseInHeader::new(80, FuseOpcode::FuseReaddir as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 80];
            fusein.write_to(&mut fusebuf);
            let readin = FuseReadIn::new(18446603336224349984, 0, 0, 0, 0, 0x18800);
            readin.write_to(&mut fusebuf[40..]);
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

            let mut outbuf = [0; 120];

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                info!("Fusevec back to readdir: {:?}", vec);
                outbuf[0..vec.len()].copy_from_slice(&vec);
                vec.clear();
            }

            let fuseout = FuseOutHeader::read_from(&outbuf);
            fuseout.print();

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

    // FuseRename = 12
    fn rename(&self, src_path: &str, dst_path: &str) -> VfsResult {
        self.check_init();
        info!("\nNEW FUSE REQUEST:\n  fuse_node RENAME from {:?} to {:?} here...", src_path, dst_path);

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            UNIQUE_ID += 2;
            let pid = current().id().as_u64();
            info!("pid = {:?}", pid);
            let fusein = FuseInHeader::new(48, FuseOpcode::FuseRename as u32, UNIQUE_ID, 1, 1000, 1000, pid as u32, 0);
            let mut fusebuf = [0; 48];
            fusein.write_to(&mut fusebuf);
            let renamein = FuseRenameIn::new(0);
            renamein.write_to(&mut fusebuf[40..]);
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
            
            FUSEFLAG.store(0, Ordering::Relaxed);
        }

        info!("fuse_node rename from {:?} to {:?} finish successfully...", src_path, dst_path);

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