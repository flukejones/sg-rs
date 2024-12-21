extern crate sg;

use sg::{Device, Task};
use std::ffi::OsStr;

static ENE_APPLY_VAL: u8 = 0x01; /* Value for Apply Changes Register     */
static ENE_SAVE_VAL: u8 = 0xAA;

static ENE_REG_MODE: u32 = 0x8021; /* Mode Selection Register              */
static ENE_REG_SPEED: u32 = 0x8022; /* Speed Control Register               */
static ENE_REG_DIRECTION: u32 = 0x8023; /* Direction Control Register           */

static ENE_REG_COLORS_DIRECT_V2: u32 = 0x8100; // to read the colurs
static ENE_REG_APPLY: u32 = 0x80A0;
static ENE_REG_COLORS_EFFECT_V2: u32 = 0x8160;
static ENE_CONFIG_CHANNEL_V2: u32 = 0x1B; /* LED Channel V2 configuration offset  */

fn array(reg: u32) -> [u8; 16] {
    let mut cdb = [0u8; 16];
    cdb[0] = 0xEC;
    cdb[1] = 0x41;
    cdb[2] = 0x53;
    cdb[3] = ((reg >> 8) & 0x00FF) as u8;
    cdb[4] = (reg & 0x00FF) as u8;
    cdb[5] = 0x00;
    cdb[6] = 0x00;
    cdb[7] = 0x00;
    cdb[8] = 0x00;
    cdb[9] = 0x00;
    cdb[10] = 0x00;
    cdb[11] = 0x00;
    cdb[12] = 0x00;
    cdb[13] = 0x04; ////////// packet_sz;
    cdb[14] = 0x00;
    cdb[15] = 0x00;
    cdb
}

fn apply_task() -> Task {
    let mut task = Task::new();
    task.set_cdb(array(ENE_REG_APPLY).as_slice());
    task.set_data(&[ENE_APPLY_VAL], sg::Direction::ToDevice);
    task
}

fn save_task() -> Task {
    let mut task = Task::new();
    task.set_cdb(array(ENE_REG_APPLY).as_slice());
    task.set_data(&[ENE_SAVE_VAL], sg::Direction::ToDevice);
    task
}

fn rgb_task(led: u32, rgb: &[u8; 3]) -> Task {
    let mut task = Task::new();
    task.set_cdb(array(led * 3 + ENE_REG_COLORS_EFFECT_V2).as_slice());
    task.set_data(rgb, sg::Direction::ToDevice);
    task
}

/// 0-13
fn mode_task(mode: u8) -> Task {
    let mut task = Task::new();
    task.set_cdb(array(ENE_REG_MODE).as_slice());
    task.set_data(&[mode.min(13)], sg::Direction::ToDevice);
    task
}

/// 0-4, fast to slow
fn speed_task(speed: u8) -> Task {
    let mut task = Task::new();
    task.set_cdb(array(ENE_REG_SPEED).as_slice());
    task.set_data(&[speed.min(4)], sg::Direction::ToDevice);
    task
}

/// 0 = forward, 1 = backward
fn dir_task(mode: u8) -> Task {
    let mut task = Task::new();
    task.set_cdb(array(ENE_REG_DIRECTION).as_slice());
    task.set_data(&[mode.min(1)], sg::Direction::ToDevice);
    task
}

fn run_tur(path: &OsStr) -> std::io::Result<()> {
    let device = Device::open(path)?;

    // can't set dir if static
    for i in 0..4 {
        let task = rgb_task(i, &[20, 0, 0]);
        device.perform(&task)?;
    }
    let task = apply_task();
    device.perform(&task)?;
    let task = save_task();
    device.perform(&task)?;

    // allows colour select: 1, 2, 3, 7, 9
    // allows dir: 5, 7, 8, 10 !12 !13
    // allows speed: !1,
    let task = mode_task(1);
    device.perform(&task)?;

    let task = speed_task(4);
    device.perform(&task)?;

    // let task = dir_task(0);
    // device.perform(&task)?;

    let task = apply_task();
    device.perform(&task)?;

    let task = save_task();
    device.perform(&task)?;

    Ok(())
}

fn main() {
    let mut args = std::env::args_os();
    if args.len() != 2 {
        eprintln!("Usage: {:?} DEV", args.next().unwrap());
        return;
    }

    if let Err(e) = run_tur(&args.next_back().unwrap()) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
