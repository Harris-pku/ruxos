/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

//! [RuxOS](https://github.com/syswonder/ruxos) fuse module.

#![cfg_attr(all(not(test), not(doc)), no_std)]
#![cfg(any(feature = "virtio-9p", feature = "net-9p"))]

#[macro_use]
extern crate log;
extern crate alloc;

pub mod fuse;
pub mod fusedev;
pub mod fuse_st;

use fuse::FuseFS;
use alloc::sync::Arc;
use log::*;

use alloc::{string::String, vec::Vec};
use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use axfs_vfs::{
    VfsDirEntry, VfsError, VfsNodeAttr, VfsNodeOps, VfsNodePerm, VfsNodeRef, VfsNodeType, VfsOps,
    VfsResult,
};
use spin::{once::Once, RwLock};

// use alloc::sync::Arc;
use log::*;
use ruxfs::{MountPoint, mounts};
use ruxdriver::{prelude::*, AxDeviceContainer};

pub fn fusefs() -> Arc<FuseFS> {
    info!("fusefs newfs here...");
    Arc::new(FuseFS::new())
}