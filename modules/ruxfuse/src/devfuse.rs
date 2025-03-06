/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

use core::sync::atomic::{AtomicI32, Ordering};
use alloc::sync::Arc;

use axfs_vfs::{VfsNodeAttr, VfsNodeOps, VfsNodePerm, VfsNodeType, VfsResult};
use log::{info, debug};
use spin::Mutex;
use spinlock::SpinNoIrq;
use alloc::vec::Vec;
use alloc::vec;

use crate::fuse_st::{FuseInHeader, FuseInitIn, FuseInitOut, FuseOutHeader};

pub static FUSEFLAG: AtomicI32 = AtomicI32::new(0);
pub static mut FUSE_VEC: Option<Arc<SpinNoIrq<Vec<u8>>>> = None;


/// A device behaves like `/dev/fuse`.
///
/// It always transmits to the daemon in user space.
pub struct FuseDev {
    data: Mutex<Vec<u8>>,
}

impl FuseDev {
    /// Create a new instance.
    pub fn new() -> Self {
        info!("fuse_dev new here...");
        Self {
            data: Mutex::new(vec![0; 1e8 as usize]),
        }
    }
}

impl VfsNodeOps for FuseDev {
    fn open(&self) -> VfsResult {
        info!("fuse_dev open here...");
        Ok(())
    }

    fn get_attr(&self) -> VfsResult<VfsNodeAttr> {
        info!("fuse_dev get_attr here...");
        Ok(VfsNodeAttr::new(
            VfsNodePerm::default_file(),
            VfsNodeType::CharDevice,
            0,
            0,
        ))
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        // info!("fuse_dev111 read buf len: {:?} at pos: {:?}", buf.len(), offset);
        
        // let guard = self.data.lock();
        // let data = guard.as_slice();
        // let offset = (offset % 1052672) as usize;
        // let len = buf.len();
        // buf[..len].copy_from_slice(&data[offset..offset + len]);

        // buf.fill(0);
        // let fusein = FuseInHeader::new(104, 26, 1234, 2, 0, 0, 0, 0);
        // fusein.write_to(buf);
        // let initin = FuseInitIn::new(10, 229, 0, 0, 0, [0; 11]);
        // initin.write_to(&mut buf[40..]);
        // fusein.print();
        // initin.print();

        // buf.copy_from_slice([1,2 ,3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        // ruxtask::task::yield_now();

        let mut flag = 0;

        unsafe {
            if FUSE_VEC.is_none() {
                FUSE_VEC = Some(Arc::new(SpinNoIrq::new(Vec::new())));
            }

            loop {
                flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag != 0 {
                    info!("Fuseflag _read_ is set to {:?},, exiting loop.", flag);
                    break;
                }
            }
    
            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                // let len = vec.len();
                // buf[..len].copy_from_slice(&vec[..len]);
                info!("Fusevec _read_: {:?}", vec);
                vec.clear();
            }

            FUSEFLAG.store(-flag, Ordering::Relaxed);
        }
        
        Ok(buf.len())
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        // info!("fuse_dev222 writes buf len: {:?} at pos: {:?}", buf.len(), offset);
        // if buf.len() == 16 {
        //     info!("fuse buf writes {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}", buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]);
        // }

        // let mut guard = self.data.lock();
        // let data = guard.as_mut_slice();
        // let offset = (offset % 1052672) as usize;
        // let len = buf.len();
        // data[offset..offset + len].copy_from_slice(&buf[..len]);

        // let fuseout = FuseOutHeader::read_from(buf);
        // let initout = FuseInitOut::read_from(&buf[16..]);
        // fuseout.print();
        // initout.print();

        let mut flag = 0;

        unsafe {
            loop {
                flag = FUSEFLAG.load(Ordering::SeqCst);
                if flag != 0 {
                    info!("Fuseflag _write_ is set to {:?},, exiting loop.", flag);
                    break;
                }
            }

            if let Some(vec_arc) = FUSE_VEC.as_ref() {
                let mut vec = vec_arc.lock();
                vec.extend_from_slice(&buf);
                // vec.extend_from_slice(&[0, 0, 0, 1]);
                info!("Fusevec _write_: {:?}", vec);
                vec.clear();
            }

            FUSEFLAG.store(-flag, Ordering::Relaxed);
        }
        
        Ok(buf.len())
    }

    fn truncate(&self, _size: u64) -> VfsResult {
        Ok(())
    }
 
    axfs_vfs::impl_vfs_non_dir_default! {}
}
 