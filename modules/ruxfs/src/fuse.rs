/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};

use axfs_vfs::{VfsDirEntry, VfsError, VfsResult};
use axfs_vfs::{VfsNodeAttr, VfsNodeOps, VfsNodeRef, VfsNodeType, VfsOps};
use spin::{once::Once, RwLock};
use crate::fuse_dev::fuse_in_header;

/// It implements [`axfs_vfs::VfsOps`].
pub struct FuseFS {
    parent: Once<VfsNodeRef>,
    root: Arc<FuseNode>,
}

impl FuseFS {
    /// Create a new instance.
    pub fn new() -> Self {
        info!("fusefs init...");
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
        *self.parent.write() = parent.map_or(Weak::<Self>::new() as _, Arc::downgrade);
    }

    /// Create a subdirectory at this directory.
    pub fn mkdir(self: &Arc<Self>, name: &'static str) -> Arc<Self> {
        info!("fuse_node mkdir...");
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
        info!("fuse_node init...");
    }
}

impl VfsNodeOps for FuseNode {
    fn open(&self) -> VfsResult {
        info!("fuse_node open here...");
        
        Ok(())
    }

    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        info!("fuse_node get_attr here...");
        Ok(VfsNodeAttr::new_dir(4096, 0))
    }

    fn parent(&self) -> Option<VfsNodeRef> {
        info!("fuse_node parent here...");
        self.parent.read().upgrade()
    }

    fn lookup(self: Arc<Self>, path: &str) -> VfsResult<VfsNodeRef> {
        info!("fuse_node lookup here...");
        // fuse_lookiup = 1
        let fusein = fuse_in_header::new(1052672, 1, 370, 2, 0, 0, 0, 0);
        fusein.write_to(&mut [0; 1052672]);
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

    fn read_dir(&self, start_idx: usize, dirents: &mut [VfsDirEntry]) -> VfsResult<usize> {
        info!("fuse_node read_dir here...");
        // fuse_readdir = 28
        // let fusein = fuse_in_header::new(1052672, 28, 370, 2, 0, 0, 0, 0);
        // fusein.write_to(&mut [0; 1052672]);
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

    fn create(&self, path: &str, ty: VfsNodeType) -> VfsResult {
        info!("fuse_node create here...");
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
        info!("fuse_node remove here...");
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

    axfs_vfs::impl_vfs_dir_default! {}
}

fn split_path(path: &str) -> (&str, Option<&str>) {
    let trimmed_path = path.trim_start_matches('/');
    trimmed_path.find('/').map_or((trimmed_path, None), |n| {
        (&trimmed_path[..n], Some(&trimmed_path[n + 1..]))
    })
}

pub fn fusefs() -> Arc<FuseFS> {
    info!("fusefs newfs here...");
    Arc::new(FuseFS::new())
}