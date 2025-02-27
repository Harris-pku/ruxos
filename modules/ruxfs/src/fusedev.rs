/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

 use axfs_vfs::{VfsNodeAttr, VfsNodeOps, VfsNodePerm, VfsNodeType, VfsResult};
 use log::{info, debug};
 use spin::Mutex;
 use alloc::vec::Vec;
 use alloc::vec;
 
 use crate::fuse;
 use crate::fuse_dev::fuse_in_header;
 
 /// A device behaves like `/dev/fuse`.
 ///
 /// It always transmits to the daemon in user space.
 pub struct FuseDev {
     Data: Mutex<Vec<u8>>,
     cnt: u64,
 }
 
 impl FuseDev {
     /// Create a new instance.
     pub fn new() -> Self {
         Self {
             Data: Mutex::new(vec![0; 1e8 as usize]),
             cnt: 0,
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
         info!("fuse_dev111 read buf len: {:?} at pos: {:?}", buf.len(), offset);
         
         let guard = self.Data.lock();
         let data = guard.as_slice();
         let offset = (offset % 1052672) as usize;
         let len = buf.len();
         buf[..len].copy_from_slice(&data[offset..offset + len]);
 
         // buf.fill(0);
         let fusein = fuse_in_header::new(32, 26, 37, 2, 0, 0, 0, 0);
         fusein.write_to(buf);
         // buf[0] = 128;
         // buf[4] = 3;
         // buf[8] = 34;
         // buf[9] = 12;
         // buf[16] = 1;
        //  ruxtask::task::yield_now();
        //  loop {
             
        //  }
         
         Ok(buf.len())
     }
 
     fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
         info!("fuse_dev write buf len: {:?} at pos: {:?}", buf.len(), offset);
         if buf.len() == 16 {
             info!("fuse buf writes {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}", buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]);
         }
 
         let mut guard = self.Data.lock();
         let data = guard.as_mut_slice();
         let offset = (offset % 1052672) as usize;
         let len = buf.len();
         data[offset..offset + len].copy_from_slice(&buf[..len]);
         
         Ok(buf.len())
     }
 
     fn truncate(&self, _size: u64) -> VfsResult {
         Ok(())
     }
  
     axfs_vfs::impl_vfs_non_dir_default! {}
 }
  