use crate::{GUARD, STACK};

pub const TOTAL: u32 = (GUARD + STACK) as u32;
pub const CONTROL: i32 = 0x0001_0F00;
pub const TOP: i32 = 0x0001_0EF0;

pub const READY: i8 = 0;
pub const RUNNING: i32 = 1;
pub const COMPLETED: i8 = 2;
pub const DONE: i32 = 2;
pub const CANCELLED: i8 = 3;
pub const ABORTED: i32 = 3;

pub const STATE: i32 = 0x00;
pub const BASE: i32 = 0x08;
pub const SIZE: i32 = 0x10;
pub const ENTRY: i32 = 0x18;
pub const ENVIRONMENT: i32 = 0x20;
pub const CLEANUP: i32 = 0x28;
pub const LOW: i32 = 0x30;
pub const HIGH: i32 = 0x38;
pub const PREVIOUS: i32 = 0x40;
pub const BATCH: i32 = 0x48;
pub const COUNT: i32 = 0x50;
pub const FIBER: i32 = 0x60;
pub const PARENT: i32 = 0xA0;

pub const RBX: i32 = 0x00;
pub const RBP: i32 = 0x08;
pub const R12: i32 = 0x10;
pub const R13: i32 = 0x18;
pub const R14: i32 = 0x20;
pub const R15: i32 = 0x28;
pub const RSP: i32 = 0x30;
pub const RIP: i32 = 0x38;

pub const PRCTL: u32 = 158;
pub const GROUP: u32 = 231;
pub const ASSIGN: u32 = 0x1001;
pub const QUERY: u32 = 0x1004;
pub const WRITABLE: u32 = 3;
pub const ANONYMOUS: u32 = 0x22;
pub const THRESHOLD: i32 = -4095;
pub const PANIC: u32 = 111;
