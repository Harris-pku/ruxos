/* Copyright (c) [2023] [Syswonder Community]
 *   [Ruxos] is licensed under Mulan PSL v2.
 *   You can use this software according to the terms and conditions of the Mulan PSL v2.
 *   You may obtain a copy of Mulan PSL v2 at:
 *               http://license.coscl.org.cn/MulanPSL2
 *   THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
 *   See the Mulan PSL v2 for more details.
 */
#![allow(clippy::identity_op)]
#![allow(dead_code)]

use alloc::sync::Arc;
// use alloc::vec::Vec;
use log::*;
use ruxdriver::prelude::*;
use spin::RwLock;

pub struct DrvVdaOps {
    transport: Arc<RwLock<AxBlockDevice>>,
}

impl DrvVdaOps {
    pub fn new(transport: AxBlockDevice) -> Self {
        info!("Create VDA driver");
        Self {
            transport: Arc::new(RwLock::new(transport)),
        }
    }
}