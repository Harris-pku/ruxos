/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */

use core::ffi::{c_int, c_ulong};
use alloc::string::String;

use axerrno::LinuxError;

const ARCH_SET_FS: i32 = 0x1002;

/// set thread state
pub fn sys_arch_prctl(code: c_int, addr: c_ulong) -> c_int {
    debug!("sys_arch_prctl <= code: {}, addr: {:#x}", code, addr);
    syscall_body!(sys_arch_prctl, {
        match code {
            ARCH_SET_FS => {
                unsafe {
                    ruxhal::arch::write_thread_pointer(addr as _);
                }
                Ok(0)
            }
            _ => Err(LinuxError::EINVAL),
        }
    })
}

/// Set process or thread attributes.
pub fn sys_prctl(
    option: c_int,
    arg2: *mut c_int,
    arg3: *mut c_int,
    arg4: *mut c_int,
    arg5: *mut c_int,
) -> c_int {
    warn!(
        "sys_prctl <= option: {}, arg2: {:?}, arg3: {:?}, arg4: {:?}, arg5: {:?}",
        option, arg2, arg3, arg4, arg5
    );
    syscall_body!(sys_prctl, {
        match option {
            3 => {
                // PR_GET_DUMPABLE
                warn!("PR_GET_DUMPABLE is set to: {}", 1);
                Ok(1)
            }
            15 => {
                // PR_SET_NAME
                let name = unsafe { core::slice::from_raw_parts(arg2 as *const u8, 16) };
                let name = String::from_utf8_lossy(name);
                warn!("PR_SET_NAME is set to: {}", name);
                // ruxtask::current().set_name(name.to_string());
                Ok(0)
            }
            16 => {
                // PR_GET_NAME
                let name = String::from(ruxtask::current().name());
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        name.as_ptr(),
                        arg2 as *mut u8,
                        name.len().min(16),
                    );
                }
                warn!("PR_GET_NAME is set to: {}", name);
                Ok(0)
            }
            _ => {
                warn!("PRCTL: {} is not supported", option);
                Err(LinuxError::EINVAL)
            }
        }
    })
}