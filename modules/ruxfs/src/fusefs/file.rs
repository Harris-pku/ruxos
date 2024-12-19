/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

use alloc::string::String;
use axio::Result;
use core::fmt;
 
use super::FileType;
use crate::fops;

use core::cell::RefCell;  
use core::collections::VecDeque;  
use alloc::rc::Rc;  
use core::sync::{Arc, Mutex, Condvar};  
use core::time::Duration;  
  
const FUSE_INT_REQ_BIT: u64 = 1;  
const FUSE_REQ_ID_STEP: u64 = 2;
  
#[derive(Debug)]  
struct FuseReq {  
    list: VecDeque<()>,  
    intr_entry: VecDeque<()>,  
    waitq: Condvar,  
    count: Rc<RefCell<usize>>,  
    flags: u32, 
}  

impl FuseReq {  
    fn new() -> FuseReq {  
        FuseReq {  
            list: VecDeque::new(),  
            intr_entry: VecDeque::new(),  
            waitq: Condvar::new(),  
            count: Rc::new(RefCell::new(1)),  
            flags: 1 << 0,
        }  
    }  
}

#[derive(Debug)]  
struct FuseIQueue {  
    lock: Spinlock<Mutex<()>>,  
    pending: Mutex<VecDeque<FuseReq>>,  
    forget_list_tail: Mutex<Option<Box<FuseForgetLink>>>,  
    reqctr: AtomicUsize,  
    connected: bool,  
    fasync: Vec<RawFd>,
    ops: FuseIQueueOps,  
}  

#[derive(Debug)]  
struct FuseForgetLink {  
    forget_one: FuseForgetOne,  
    next: Option<Box<FuseForgetLink>>,  
}  
  
#[derive(Debug)]  
struct FuseForgetOne {  
    nodeid: u64,  
    nlookup: u64,  
}  
  
#[derive(Debug)]  
struct FuseIQueueOps {  
    wake_forget_and_unlock: fn(&FuseIQueue),  
    wake_interrupt_and_unlock: fn(&FuseIQueue),  
    wake_pending_and_unlock: fn(&FuseIQueue),  
}
  
#[derive(Debug)]  
struct FuseConn {  
    initialized: bool,  
    blocked: bool,  
    connected: bool,  
    conn_error: bool,  
    user_ns: usize,
    pid_ns: usize,
    num_waiting: Arc<Mutex<usize>>,  
    blocked_waitq: Arc<(Mutex<()>, Condvar)>,  
}  
  
impl FuseConn {  
    fn new() -> FuseConn {  
        let num_waiting = Arc::new(Mutex::new(0));  
        let (lock, cvar) = Condvar::new();  
        let blocked_waitq = Arc::new((Mutex::new(()), cvar));  
  
        FuseConn {  
            initialized: false,  
            blocked: false,  
            connected: false,  
            conn_error: false,  
            user_ns: 0,
            pid_ns: 0,
            num_waiting,  
            blocked_waitq,  
        }  
    }  
  
    fn block_alloc(&self, for_background: bool) -> bool {  
        !self.initialized || (for_background && self.blocked)  
    }  
  
    fn drop_waiting(&self) {  
        let mut num_waiting = self.num_waiting.lock().unwrap();  
        *num_waiting -= 1;  
        if *num_waiting == 0 && !self.connected {  
            let (lock, cvar) = &*self.blocked_waitq;  
            let mut lock = lock.lock().unwrap();  
            cvar.notify_all();  
        }  
    }  
}  
  
fn fuse_get_req(fc: &FuseConn, for_background: bool) -> Result<FuseReq, i32> {
    let mut num_waiting = fc.num_waiting.lock().unwrap();  
    *num_waiting += 1;
  
    if fc.block_alloc(for_background) {
        let (lock, cvar) = &*fc.blocked_waitq;  
        let mut lock = lock.lock().unwrap();  
        while fc.block_alloc(for_background) {  
            if let Err(_) = cvar.wait_timeout(lock, Duration::from_secs(1)) {  
                return Err(-EINTR as i32);  
            }  
        }  
    }  
  
    if !fc.connected {  
        return Err(-ENOTCONN as i32);  
    }  
  
    if fc.conn_error {  
        return Err(-ECONNREFUSED as i32);  
    }

    Ok(FuseReq::new())
} 

fn fuse_len_args(numargs: usize, args: &[FuseArg]) -> usize {
    args.iter().map(|arg| arg.size).sum()  
}

impl FuseIQueue {  
    fn get_unique(&self) -> u64 {  
        let new_value = self.reqctr.fetch_add(FUSE_REQ_ID_STEP, Ordering::SeqCst) as u64;  
        new_value  
    }  
}

const FUSE_DEV_FIQ_OPS: FuseIQueueOps = FuseIQueueOps {  
    wake_forget_and_unlock: fuse_dev_wake_and_unlock,  
    wake_interrupt_and_unlock: fuse_dev_wake_and_unlock,  
    wake_pending_and_unlock: fuse_dev_wake_and_unlock,  
};

fn queue_request_and_unlock(fiq: &FuseIQueue, req: &FuseReq) {  
    let args_len = fuse_len_args(req.args.in_numargs, &req.args.in_args);  
    req.in.len = std::mem::size_of::<FuseInHeader>() + args_len;  
      
    let mut pending_guard = fiq.pending.lock().unwrap();  
    pending_guard.push_back(req.clone());
      
    fiq.ops.wake_pending_and_unlock(fiq);  
}  
  
fn fuse_queue_forget(fc: &FuseConn, forget: Box<FuseForgetLink>, nodeid: u64, nlookup: u64) {  
    let fiq = &fc.iq;  
      
    forget.forget_one.nodeid = nodeid;  
    forget.forget_one.nlookup = nlookup;  
      
    let mut lock = fiq.lock.lock().unwrap();  
    if fiq.connected {  
        let mut forget_list_tail_guard = fiq.forget_list_tail.lock().unwrap();  
        *forget_list_tail_guard = Some(forget);  
        fiq.ops.wake_forget_and_unlock(fiq);  
    }
}  
  
fn flush_bg_queue(fc: &FuseConn) {  
    let fiq = &fc.iq;  
      
    while fc.active_background < fc.max_background && !fc.bg_queue.lock().unwrap().is_empty() {  
        let mut bg_queue_guard = fc.bg_queue.lock().unwrap();  
        let req = bg_queue_guard.pop_front().unwrap();
        fc.active_background += 1;  
          
        let mut fiq_lock = fiq.lock.lock().unwrap();  
        req.in.unique = fiq.get_unique();  
        queue_request_and_unlock(fiq, &req);  
    }  
}

pub fn fuse_simple_request(fc: &FuseConn, args: &FuseArgs) -> c_int {  
    let mut req: *mut FuseReq;  
  
    if args.force {  
        fc.num_waiting.fetch_add(1, Ordering::SeqCst);  
        req = fuse_request_alloc(0);
  
        unsafe {  
            (*req).set_flag(0x01);
            (*req).set_flag(0x02);
        }  
    } else {  
        match fuse_get_req(fc, false) {  
            Ok(r) => req = r,  
            Err(e) => return e,  
        }  
    }  

    if !args.noreply {  
        unsafe {  
            (*req).set_flag(0x04);
        }  
    }  
  
    __fuse_request_send(fc, unsafe { &mut *req });  
  
    let ret = unsafe { (*req).out.error };  
  
    fuse_put_request(fc, req);  
  
    ret  
}  