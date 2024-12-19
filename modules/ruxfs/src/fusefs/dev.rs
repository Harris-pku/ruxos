/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

use super::FileType;
use crate::fops;

use core::cell::RefCell;  
use core::collections::VecDeque;  
use alloc::rc::Rc;  
use core::sync::{Arc, Mutex, Condvar};  
use core::time::Duration; 
 
fn fuse_aio_complete(io: &mut FuseIoPriv, err: c_int, pos: ssize_t) {
    let mut left = 0;  

    let _lock = io.lock.lock().unwrap();
    if err != 0 {  
        io.err = if io.err != 0 { io.err } else { err };  
    } else if pos >= 0 && (io.bytes < 0 || pos as isize < io.bytes as isize) {  
        io.bytes = pos;  
    }  
 
    left = io.reqs.wrapping_sub(1);  
    if left == 0 && io.blocking {  
        complete(&io.done);  
    }

    if left == 0 && !io.blocking {  
        let res = fuse_get_res_by_io(io);  

        if res >= 0 {  
            let inode = file_inode(io.iocb as *const c_void);  
            let fc = get_fuse_conn(inode);  
            let fi = get_fuse_inode(inode);  

            unsafe {  
                let _lock = (*(fi as *const FuseInode)).lock.lock().unwrap();  
                (*(fc as *const FuseConn)).attr_version  
                    .fetch_add(1, Ordering::SeqCst);  
                (*(fi as *mut FuseInode)).attr_version = (*(fc as *const FuseConn)).attr_version.load(Ordering::SeqCst);  
            }  
        }  
   
        unsafe {  
            ((*(io.iocb as *const FuseIocb)).ki_complete)(io.iocb, res, 0);  
        }  
    }  

    kref_put(io);  
}  
   
fn fuse_io_alloc(io: &FuseIoPriv, npages: usize) -> Option<*mut FuseIoArgs> {  
    let ia = kzalloc(std::mem::size_of::<FuseIoArgs>())?;  
    if ia.is_null() {  
        return None;  
    }

    unsafe {  
        ptr::write(ia, FuseIoArgs {  
            io: io as *const FuseIoPriv,  
            ap: FuseIoAp {  
                pages: fuse_pages_alloc(npages, 0, ptr::null_mut()),  
                descs: ptr::null(),  
            },  
        });  

        if ia.as_ref().unwrap().ap.pages.is_null() {  
            kfree(ia as *const c_void);
            return None;
        }
    }

    Some(ia)
}

fn fuse_send_readpages(  
    fc: &Arc<FuseConnection>,  
    ff: &FuseFile,  
    pos: loff_t,  
    num_pages: usize,  
) -> Result<(), Box<dyn std::error::Error>> {  
    let page_size = 4096;
    let total_size = num_pages * page_size;  
    let mut buffer: Vec<u8> = vec![0; total_size];  
  
    let read_args = FuseReadArgs {  
        fh: ff.fd as c_int,  
        offset: pos,  
        size: total_size as size_t,  
    };  

    let result = unsafe {  
        let ptr = buffer.as_mut_ptr() as *mut c_void;  
        read(ff.fd, ptr, total_size)?  
    };  
   
    if result < 0 {  
        return Err(format!("Read error: {}", result).into());  
    }
   
    Ok(())  
}

fn fuse_send_open(
    fc: &mut FuseConn,
    nodeid: u64,
    file: &File,
    opcode: i32,
    outargp: &mut FuseOpenOut,
) -> Result<(), i32> {
    let mut inarg = FuseOpenIn::default();
    inarg.flags = file.f_flags & !(O_CREAT | O_EXCL | O_NOCTTY);

    if !fc.atomic_o_trunc {
        inarg.flags &= !O_TRUNC;
    }

    let mut args = FuseArgs::default();
    args.opcode = opcode;
    args.nodeid = nodeid;
    args.in_numargs = 1;
    args.in_args[0].size = std::mem::size_of::<FuseOpenIn>() as u32;
    args.in_args[0].value = &inarg as *const _ as *const std::ffi::c_void;

    args.out_numargs = 1;
    args.out_args[0].size = std::mem::size_of::<FuseOpenOut>() as u32;
    args.out_args[0].value = outargp as *mut _ as *mut std::ffi::c_void;

    fuse_simple_request(fc, &args)
}

fn fuse_open_common(inode: &mut Inode, file: &mut File, isdir: bool) -> Result<(), i32> {
    let fc = get_fuse_conn(inode);
    let is_wb_truncate = (file.f_flags & O_TRUNC != 0) && fc.atomic_o_trunc && fc.writeback_cache;

    let err = generic_file_open(inode, file)?;
    
    if is_wb_truncate {
        inode_lock(inode);
        fuse_set_nowrite(inode);
    }

    let err = fuse_do_open(fc, get_node_id(inode), file, isdir);

    if err.is_ok() {
        fuse_finish_open(inode, file);
    }

    if is_wb_truncate {
        fuse_release_nowrite(inode);
        inode_unlock(inode);
    }

    err
}

fn fuse_do_open(
    fc: &mut FuseConn,
    nodeid: u64,
    file: &mut File,
    isdir: bool,
) -> Result<(), i32> {
    let mut ff = FuseFile::alloc(fc).ok_or(-ENOMEM)?;

    ff.fh = 0;
    ff.open_flags = FOPEN_KEEP_CACHE | if isdir { FOPEN_CACHE_DIR } else { 0 };

    let opcode = if isdir { FUSE_OPENDIR } else { FUSE_OPEN };

    if (isdir && !fc.no_opendir) || (!isdir && !fc.no_open) {
        let mut outarg = FuseOpenOut::default();
        let err = fuse_send_open(fc, nodeid, file, opcode, &mut outarg);

        if err.is_ok() {
            ff.fh = outarg.fh;
            ff.open_flags = outarg.open_flags;
        } else if err != -ENOSYS {
            fuse_file_free(ff);
            return Err(err);
        } else {
            if isdir {
                fc.no_opendir = true;
            } else {
                fc.no_open = true;
            }
        }
    }

    if isdir {
        ff.open_flags &= !FOPEN_DIRECT_IO;
    }

    ff.nodeid = nodeid;
    file.private_data = Some(ff);

    Ok(())
}