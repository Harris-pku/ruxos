/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

//! VDA filesystem used by [Ruxos](https://github.com/syswonder/ruxos).
//!
//! The implementation is based on [`axfs_vfs`].

#![allow(dead_code)]

use alloc::{sync::Arc, sync::Weak};
use alloc::vec;
use axfs_vfs::{
    VfsNodeAttr, VfsNodeOps, VfsNodePerm, VfsNodeRef, VfsNodeType, VfsOps, VfsResult
};
use log::*;
use ruxdriver::AxBlockDevice;
use ruxdriver::prelude::BlockDriverOps;
use spin::{once::Once, RwLock};

/// A VDA filesystem that implements [`axfs_vfs::VfsOps`].
pub struct VdaFileSystem {
    parent: Once<VfsNodeRef>,
    root: Arc<VdaNode>,
}

impl VdaFileSystem {
    /// Create a new VDA filesystem.
    pub fn new(dev: AxBlockDevice) -> Self {
        info!("Create VDA filesystem");
        Self {
            parent: Once::new(),
            root: VdaNode::new(None, dev),
        }
    }
}

impl VfsOps for VdaFileSystem {
    fn mount(&self, _path: &str, _mountpoint: VfsNodeRef) -> VfsResult {
        info!("Mount VDA filesystem");
        Ok(())
    }

    fn root_dir(&self) -> VfsNodeRef {
        info!("Get VDA root directory");
        self.root.clone()
    }
}

pub struct VdaNode {
    this: Weak<VdaNode>,
    parent: RwLock<Weak<dyn VfsNodeOps>>,
    transport: Arc<RwLock<AxBlockDevice>>,
}

impl VdaNode {
    pub(super) fn new(parent: Option<Weak<dyn VfsNodeOps>>, dev: AxBlockDevice) -> Arc<Self> {
        info!("Create VDA node");
        Arc::new_cyclic(|this| Self {
            this: this.clone(),
            parent: RwLock::new(parent.unwrap_or_else(|| Weak::<Self>::new())),
            transport: Arc::new(RwLock::new(dev)),
        })
    }
}

impl VfsNodeOps for VdaNode {
    fn open(&self) -> VfsResult {
        info!("Open VDA node");
        Ok(())
    }

    fn lookup(self: Arc<Self>, path: &str) -> VfsResult<VfsNodeRef> {
        info!("Lookup VDA node {:?}", path);
        Ok(self)
    }

    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        info!("Get VDA node attributes");
        Ok(VfsNodeAttr::new(
            VfsNodePerm::from_bits_truncate(0o777),
            VfsNodeType::BlockDevice,
            67108864,
            131072
        ))
        // Ok(VfsNodeAttr::new_file(67108864, 131072))
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        info!("Read VDA node offset: {:?}, buf: {:?}", offset, buf.len());

        let mut dev = self.transport.write();
        let mut temp_buf = vec![0u8; 512];
        let ret = dev.read_block(offset/512, &mut temp_buf);
        buf.copy_from_slice(&temp_buf[(offset%512)  as usize..(offset%512 )as usize+buf.len()]);

        Ok(buf.len())
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        info!("Write VDA node");
        info!("Write VDA node offset: {:?}, buf: {:?}", offset, buf.len());
        Ok(0)
    }
}