//! Constant values and static structures.
use lazy_static;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::{ONCE_INIT,Once};
use std::sync::atomic::compiler_fence;

use pleco::tools::tt::TranspositionTable;
use pleco::helper::prelude;
//use time::time_management::TimeManager;


use threadpool;

pub const MAX_PLY: u16 = 126;
pub const THREAD_STACK_SIZE: usize = MAX_PLY as usize + 7;
pub const MAX_THREADS: usize = 256;

pub const DEFAULT_TT_SIZE: usize = 256;

pub const PAWN_TABLE_SIZE: usize = 16384;
pub const MATERIAL_TABLE_SIZE: usize = 8192;

static INITALIZED: Once = ONCE_INIT;

pub static USE_STDOUT: AtomicBool = AtomicBool::new(true);
/// Global Timer
//pub static TIMER: TimeManager = TimeManager::uninitialized();

lazy_static! {
    pub static ref TT_TABLE: TranspositionTable = TranspositionTable::new(DEFAULT_TT_SIZE);
}

pub fn init_globals() {
    INITALIZED.call_once(|| {
        prelude::init_statics();
        compiler_fence(Ordering::SeqCst);
        lazy_static::initialize(&TT_TABLE);
        threadpool::init_threadpool();

    });
}



pub trait PVNode {
    fn is_pv() -> bool;
}

pub struct PV {}
pub struct NonPV {}

impl PVNode for PV {
    fn is_pv() -> bool {
        true
    }
}

impl PVNode for NonPV {
    fn is_pv() -> bool {
        false
    }
}

pub trait CheckState {
    fn in_check() -> bool;
}


pub struct InCheck {}
pub struct NoCheck {}

impl CheckState for InCheck {
    fn in_check() -> bool { true}
}

impl CheckState for NoCheck {
    fn in_check() -> bool { false}
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_da() {
        threadpool::init_threadpool();

    }
}