extern crate sg;

use sg::{Device, Task};
use std::ffi::OsStr;
use std::time::Duration;

fn run_tur(path: &OsStr) -> std::io::Result<()> {
    let cdb = [0; 6];
    let mut task = Task::new();
    task.set_timeout(Duration::from_secs(20));
    task.set_cdb(&cdb);
    let mut device = Device::open(path)?;
    assert_eq!(device.send(&[task])?, 1);
    let mut tasks = Vec::new();
    assert_eq!(device.receive(&mut tasks)?, 1);
    println!("{}", tasks[0].ok());
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
