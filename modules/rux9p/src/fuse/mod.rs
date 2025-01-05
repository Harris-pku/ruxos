/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

use alloc::{string::String, string::ToString, sync::Arc, sync::Weak, vec::Vec};
use axfs_vfs::{
    VfsDirEntry, VfsError, VfsNodeAttr, VfsNodeOps, VfsNodePerm, VfsNodeRef, VfsNodeType, VfsOps,
    VfsResult,
};
use log::*;
use spin::{once::Once, RwLock};
use ruxdriver::{prelude::*, AxDeviceContainer};
use ruxfs::MountPoint;

pub fn init_dev_fuse(
    mut devs: AxDeviceContainer<Ax9pDevice>,
    aname: &str,
    protocol: &str,
) -> MountPoint {
// ) -> u32 {
    info!("Initialize dev fuse...");

    let vfuse = devs.take_one().expect("No fusefs device found!");
    info!("  use fusefs device 0: {:?}", vfuse.device_name());

    // let vfuse_driver = DrvFuseOps::new(vfuse);
    // let vfuse_fs = FuseFileSystem::new(Arc::new(RwLock::new(vfuse_driver)), aname, protocol);
    let vfuse_driver = crate::drv::Drv9pOps::new(vfuse);
    let vfuse_fs = crate::fs::_9pFileSystem::new(Arc::new(RwLock::new(vfuse_driver)), aname, protocol);

    MountPoint::new("/dev/fuse", Arc::new(vfuse_fs))
}
