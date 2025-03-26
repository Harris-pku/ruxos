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

const BLOCK_SIZE: usize = 512;

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
    fn mount(&self, path: &str, mount_point: VfsNodeRef) -> VfsResult {
        debug!("Mount VDA filesystem, path: {:?}", path);
        if let Some(parent) = mount_point.parent() {
            self.root.set_parent(Some(self.parent.call_once(|| parent)));
        } else {
            self.root.set_parent(None);
        }
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

    pub(super) fn set_parent(&self, parent: Option<&VfsNodeRef>) {
        *self.parent.write() = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
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
        info!(
            "Read VDA node offset: {:?}, % = {:?}, buf_len: {:?}, last: {:?}",
            offset,
            offset % BLOCK_SIZE as u64,
            buf.len(),
            (offset + buf.len() as u64) % BLOCK_SIZE as u64
        );
    
        let mut dev = self.transport.write();
        let mut cur_offset = offset;
        let mut pos = 0;
        let mut remain = buf.len();
        let mut temp_buf = vec![0u8; BLOCK_SIZE];
    
        while remain > 0 {
            let ret = dev.read_block(cur_offset / 512, &mut temp_buf);
    
            let copy_len = remain.min(BLOCK_SIZE as usize);
            let start = cur_offset as usize % 512;
            let end = start + copy_len;
    
            buf[pos..pos + copy_len].copy_from_slice(&temp_buf[start..end]);
    
            info!("copy_len: {:?}, cur_offset: {:?}, pos: {:?}, remain: {:?}", copy_len, cur_offset, pos, remain);
    
            cur_offset += copy_len as u64;
            remain -= copy_len;
            pos += copy_len;
        }

        if buf.len() == 4 {
            // buf.fill(377);
            info!("buf: {:?}", buf);
        }
    
        Ok(buf.len())
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        info!("Write VDA node offset: {:?}, buf: {:?}", offset, buf.len());
        // info!("buf: {:?}", buf);

        let mut dev = self.transport.write();
        let mut cur_offset = offset;
        let mut pos = 0;
        let mut remain = buf.len();

        while remain > 0 {
            let mut temp_buf = vec![0u8; BLOCK_SIZE];
            let copy_len = remain.min(BLOCK_SIZE as usize);
            temp_buf[(cur_offset as usize % BLOCK_SIZE)..(cur_offset as usize % BLOCK_SIZE) + copy_len].copy_from_slice(&buf[pos..pos+copy_len]);
            let _ret = dev.write_block(cur_offset / BLOCK_SIZE as u64, &temp_buf);
            cur_offset += copy_len as u64;
            remain -= copy_len;
            pos += copy_len;
        }

        Ok(buf.len())
    }
}