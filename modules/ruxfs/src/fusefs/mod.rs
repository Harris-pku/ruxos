/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

// use alloc::{string::String, vec::Vec};
// use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
// use core::sync::Mutex;
use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::RefCell;
use alloc::rc::Rc;
// use axfs_vfs::{
//     VfsDirEntry, VfsError, VfsNodeAttr, VfsNodeOps, VfsNodePerm, VfsNodeRef, VfsNodeType, VfsOps,
//     VfsResult,
// };
use spin::{once::Once, RwLock};

use log::*;
// use super::{MountPoint, mounts};
use ruxdriver::{prelude::*, AxDeviceContainer};

use core::{
    alloc::Layout,
    ffi::{c_int, c_uint, c_void, c_short},
};


pub fn test_fuse(one: u32) {
    info!("test fuse at {:#}", one);
}

// static FUSE_CONN_LIST: Mutex<ListHead> = Mutex::new(ListHead::new());
// static FUSE_CONN_LIST: RwLock<ListHead> = RwLock::new(ListHead::new());

pub fn init_fusefs(_fuse_devs: AxDeviceContainer<AxBlockDevice>) {
    info!("Initialize fusefs...");

    // let fuse = fuse_devs.take_one().expect("No fusefs device found!");
    // info!("  use fusefs device 0: {:?}", fuse.device_name());

    // MountPoint::new("/", mounts::ramfs())

    // init_list_head(FUSE_CONN_LIST);
    let mut res: u32 = fuse_fs_init();
    let mut res: u32 = fuse_dev_init();
    let mut res: u32 = fuse_sysfs_init();
    let mut res: u32 = fuse_ctl_init();
}

pub fn fuse_fs_init() -> u32 {
    // create slob cache
    // kmem_cache_create()

    // register_fuseblk()
    // register_filesystem()
    return 0;
}

pub fn fuse_dev_init() -> u32 {

    return 1;
}

pub fn fuse_sysfs_init() -> u32 {

    return 1;
}

pub fn fuse_ctl_init() -> u32 {

    return 0;
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ListHead {
    pub next: *mut ListHead,
    pub prev: *mut ListHead,
}

impl ListHead {
    pub fn new() -> Self {
        ListHead {
            next: core::ptr::null_mut(),
            prev: core::ptr::null_mut(),
        }
    }

    pub fn init(&mut self) {
        self.next = self;
        self.prev = self;
    }
}
 
pub fn init_list_head(list: &mut ListHead) {
    list.next = list;
    list.prev = list;
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct FuseInHeader {
    pub len: u32,
    pub opcode: u32,
    pub unique: u64,
    pub nodeid: u64,
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
    pub padding: u32,
}
 
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct FuseOutHeader {
    pub len: u32,
    pub error: i32,
    pub unique: u64,
}

#[derive(Debug, Copy, Clone)]
struct FuseForgetOne {
    nodeid: u64,
    nlookup: u64,
}

#[derive(Debug)]
struct FuseForgetLink {
    forget_one: FuseForgetOne,
    next: Option<Rc<RefCell<FuseForgetLink>>>,
}

#[derive(Debug)]
pub struct FuseReq {
    list: ListHead,
    intr_entry: ListHead,
    args: Arc<FuseArgs>,
    count: u64,
    flags: u64,
    in_header: FuseInHeader,
    out_header: FuseOutHeader,
    waitq: u64,
    #[cfg(feature = "virtio_fs")]
    argbuf: Option<*const c_void>,
}

/// One input argument of a request
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct FuseInArg {
    pub size: c_uint,
    pub value: *const c_void,
}
 
/// One output argument of a request
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct FuseArg {
    pub size: c_uint,
    pub value: *mut c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct FuseArgs {
    pub nodeid: u64,
    pub opcode: u32,
    pub in_numargs: c_short,
    pub out_numargs: c_short,
    
    pub force: u8,
    pub noreply: u8,
    pub nocreds: u8,
    pub in_pages: u8,
    pub out_pages: u8,
    pub out_argvar: u8,
    pub page_zeroing: u8,
    pub page_replace: u8,
    
    pub in_args: [FuseInArg; 3],
    pub out_args: [FuseArg; 2],
    
    pub end: extern "C" fn(*mut FuseConn, *mut FuseArgs, i32),
}

#[derive(Debug)]
struct FuseIQueue {
    connected: u8,
    // lock: Arc<(Mutex<()>, Condvar<()>)>,
    // waitq: Arc<Mutex<VecDeque<RawFd>>>,
    reqctr: u64,
    pending: ListHead,
    interrupts: ListHead,
    // forget_list_head: FuseForgetLink,
    // forget_list_tail: Option<Box<FuseForgetLink>>,
    forget_batch: i32,
    // fasync: Option<Box<dyn Fasync>>,
    // ops: Box<dyn FuseIQueueOps>,
    // priv: Box<dyn Any>,
}

#[derive(Debug)]
pub struct FuseConn {
    // lock: Arc<Spinlock<()>>,
    // count: Arc<AtomicUsize>,
    // dev_count: Arc<AtomicUsize>,
    // rcu: RCUHead,
    user_id: u32,
    group_id: u32,
    // pid_ns: PhantomData<*const ()>,
    // user_ns: PhantomData<*const ()>,
    max_read: usize,
    max_write: usize,
    max_pages: usize,
    // iq: FuseIQueue,
    // khctr: Arc<AtomicU64>,
    // polled_files: RBRoot<PhantomData<*const ()>>,
    max_background: usize,
    congestion_threshold: usize,
    num_background: usize,
    active_background: usize,
    // bg_queue: Vec<PhantomData<*const ()>>,
    // bg_lock: Arc<Spinlock<()>>,
    // initialized: Arc<AtomicBool>,
    // blocked: Arc<AtomicBool>,
    // blocked_waitq: PhantomData<*const ()>,
    connected: usize,
    aborted: u8,
    conn_error: u8,
    conn_init: u8,
    async_read: u8,
    abort_err: u8,
    atomic_o_trunc: u8,
    export_support: u8,
    writeback_cache: u8,
    parallel_dirops: u8,
    handle_killpriv: u8,
    cache_symlinks: u8,

    no_open: u8,
    no_opendir: u8,
    no_fsync: u8,
    no_fsyncdir: u8,
    no_flush: u8,
    no_setxattr: u8,
    no_getxattr: u8,
    no_listxattr: u8,
    no_removexattr: u8,
    no_lock: u8,
    no_access: u8,
    no_create: u8,
    no_interrupt: u8,
    no_bmap: u8,
    no_poll: u8,
    big_writes: u8,
    dont_mask: u8,
    no_flock: u8,
    no_fallocate: u8,
    no_rename2: u8,
    auto_inval_data: u8,
    explicit_inval_data: u8,
    do_readdirplus: u8,
    readdirplus_auto: u8,
    async_dio: u8,
    no_lseek: u8,
    posix_acl: u8,
    default_permissions: u8,
    allow_other: u8,
    no_copy_file_range: u8,
    destroy: u8,
    delete_stale: u8,
    no_control: u8,
    no_force_umount: u8,
    no_mount_options: u8,
    // num_waiting: Arc<AtomicUsize>,
    // minor: u32,
    // entry: PhantomData<*const ()>,
    // dev: u64,
    // ctl_dentry: [Option<Dentry>; 10],
    // ctl_ndents: usize,
    // scramble_key: [u32; 4],
    // attr_version: Arc<AtomicU64>,
    // release: Option<Box<dyn Fn(Arc<FuseConn>) + Send + Sync>>,
    // sb: SuperBlock,
    // killsb: RwLock<()>,
    devices: ListHead,
}