/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

#![allow(dead_code)]
#![allow(unused_imports)]
use core::sync::atomic::{AtomicI32, Ordering};
use alloc::sync::Arc;

use axfs_vfs::{VfsNodeAttr, VfsNodeOps, VfsNodePerm, VfsNodeType, VfsResult};
use log::*;
use spin::Mutex;
use spinlock::SpinNoIrq;
use alloc::vec::Vec;
use alloc::vec;


/// A exfat blk device `/dev/sda`.
///
/// exfat block device
pub struct SdaDev {
    data: Mutex<Vec<u8>>,
    // drvsdaops
}

impl SdaDev {
    /// Create a new instance.
    pub fn new() -> Self {
        info!("exfat new here...");
        Self {
            data: Mutex::new(vec![0; 1e8 as usize]),
        }
    }
}

impl VfsNodeOps for SdaDev {
    fn open(&self) -> VfsResult {
        info!("exfat open here...");
        Ok(())
    }

    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        info!("exfat get_attr here...");
        Ok(VfsNodeAttr::new(
            VfsNodePerm::default_file(),
            VfsNodeType::CharDevice,
            0,
            0,
        ))
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        info!("exfat read buf len: {:?} at pos: {:?}", buf.len(), offset);
        
        Ok(buf.len())
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        info!("exfat writes buf len: {:?} at pos: {:?}, buf: {:?}", buf.len(), offset, buf);

        Ok(buf.len())
    }

    fn truncate(&self, _size: u64) -> VfsResult {
        Ok(())
    }
 
    axfs_vfs::impl_vfs_non_dir_default! {}
}
 