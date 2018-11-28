#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

extern crate libc;
#[cfg(feature = "polling")]
extern crate mio;
extern crate nix;

#[cfg(feature = "polling")]
use mio::event::Evented;
#[cfg(feature = "polling")]
use mio::unix::EventedFd;
#[cfg(feature = "polling")]
use mio::{Poll, PollOpt, Ready, Token};
use nix::sys::uio;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::time::Duration;

///
pub mod sys {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
    pub const SG_FLAG_Q_AT_TAIL: u32 = 0x10;
}

///
#[derive(Debug, Copy, Clone)]
pub enum Direction {
    None,
    ToDevice,
    FromDevice,
    ToFromDevice,
}

impl Direction {
    fn to_underlying(self) -> std::os::raw::c_int {
        match self {
            Direction::None => sys::SG_DXFER_NONE,
            Direction::ToDevice => sys::SG_DXFER_TO_DEV,
            Direction::FromDevice => sys::SG_DXFER_FROM_DEV,
            Direction::ToFromDevice => sys::SG_DXFER_TO_FROM_DEV,
        }
    }
}

///
#[derive(Copy, Clone, Debug, Default)]
pub struct Task(sys::sg_io_hdr);

impl Task {
    ///
    pub fn new() -> Self {
        Task(sys::sg_io_hdr {
            interface_id: 'S' as std::os::raw::c_int,
            dxfer_direction: sys::SG_DXFER_NONE,
            ..Default::default()
        })
    }

    fn from_underlying(sg: sys::sg_io_hdr) -> Self {
        Task(sg)
    }

    ///
    pub fn set_cdb(&mut self, buf: &[u8]) -> &mut Self {
        self.0.cmdp = buf.as_ptr() as *mut u8;
        self.0.cmd_len = buf.len() as u8;
        self
    }

    ///
    pub fn cdb(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.0.cmdp, self.0.cmd_len as usize) }
    }

    ///
    pub fn set_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.0.timeout =
            (timeout.as_secs() * 1_000 + (u64::from(timeout.subsec_nanos()) / 1_000_000)) as u32;
        self
    }

    ///
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.0.timeout.into())
    }

    ///
    pub fn set_data(&mut self, buf: &[u8], direction: Direction) -> &mut Self {
        self.0.dxferp = buf.as_ptr() as *mut std::os::raw::c_void;
        self.0.dxfer_len = buf.len() as u32;
        self.0.dxfer_direction = direction.to_underlying();
        self
    }

    ///
    pub fn set_data_mut(&mut self, buf: &mut [u8], direction: Direction) -> &mut Self {
        self.0.dxferp = buf.as_ptr() as *mut std::os::raw::c_void;
        self.0.dxfer_len = buf.len() as u32;
        self.0.dxfer_direction = direction.to_underlying();
        self
    }

    ///
    pub fn data(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.0.dxferp as *const u8, self.0.dxfer_len as usize) }
    }

    ///
    pub fn data_mut(&mut self) -> &mut [u8] {
        unsafe {
            std::slice::from_raw_parts_mut(self.0.dxferp as *mut u8, self.0.dxfer_len as usize)
        }
    }

    ///
    pub fn set_sense_buffer(&mut self, buf: &[u8]) -> &mut Self {
        self.0.sbp = buf.as_ptr() as *mut u8;
        self.0.mx_sb_len = buf.len() as u8;
        self
    }

    ///
    pub fn sense_buffer(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.0.sbp, self.0.sb_len_wr as usize) }
    }

    ///
    pub fn set_flags(&mut self, flags: u32) -> &mut Self {
        self.0.flags = flags;
        self
    }

    ///
    pub fn flags(&self) -> u32 {
        self.0.flags
    }

    ///
    pub fn set_usr_ptr(&mut self, ptr: *const std::os::raw::c_void) -> &mut Self {
        self.0.usr_ptr = ptr as *mut std::os::raw::c_void;
        self
    }

    ///
    pub fn usr_ptr(&self) -> *const std::os::raw::c_void {
        self.0.usr_ptr as *const std::os::raw::c_void
    }

    ///
    pub fn duration(&self) -> u32 {
        self.0.duration
    }

    ///
    pub fn residual_data(&self) -> i32 {
        self.0.resid
    }

    ///
    pub fn status(&self) -> u8 {
        self.0.status
    }

    ///
    pub fn host_status(&self) -> u16 {
        self.0.host_status
    }

    ///
    pub fn driver_status(&self) -> u16 {
        self.0.driver_status
    }

    ///
    pub fn ok(&self) -> bool {
        (self.0.info & sys::SG_INFO_OK_MASK) == sys::SG_INFO_OK
    }
}

///
pub struct Device(File);

impl Device {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Device> {
        Ok(Device(
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(path)?,
        ))
    }

    /// Returns the number of tasks successfully sent.
    pub fn send(&self, tasks: &[Task]) -> io::Result<usize> {
        if tasks.is_empty() {
            return Ok(0);
        }

        let mut iovecs: [uio::IoVec<&[u8]>; sys::SG_MAX_QUEUE as usize] =
            unsafe { std::mem::uninitialized() };
        for (task, mut iovec) in tasks.iter().zip(iovecs.iter_mut()) {
            *iovec = uio::IoVec::from_slice(unsafe {
                std::slice::from_raw_parts(
                    &task.0 as *const sys::sg_io_hdr as *const u8,
                    std::mem::size_of::<sys::sg_io_hdr>(),
                )
            });
        }

        loop {
            match uio::writev(self.0.as_raw_fd(), &iovecs[..tasks.len()]) {
                Ok(n) => break Ok(n / std::mem::size_of::<sys::sg_io_hdr>()),
                Err(nix::Error::Sys(ref e)) if e == &nix::errno::Errno::EINTR => {}
                Err(nix::Error::Sys(e)) => break Err(e.into()),
                _ => unreachable!(),
            }
        }
    }

    /// Returns the number of tasks received - how many were added to `tasks`.
    pub fn receive(&self, tasks: &mut Vec<Task>) -> io::Result<usize> {
        let mut hdrs: [sys::sg_io_hdr; sys::SG_MAX_QUEUE as usize] =
            unsafe { std::mem::uninitialized() };
        let mut iovecs: [uio::IoVec<&mut [u8]>; sys::SG_MAX_QUEUE as usize] =
            unsafe { std::mem::uninitialized() };

        for (mut hdr, mut iovec) in hdrs.iter_mut().zip(iovecs.iter_mut()) {
            *iovec = uio::IoVec::from_mut_slice(unsafe {
                std::slice::from_raw_parts_mut(
                    hdr as *mut sys::sg_io_hdr as *mut u8,
                    std::mem::size_of::<sys::sg_io_hdr>(),
                )
            });
        }

        let bytes_read = loop {
            match uio::readv(self.0.as_raw_fd(), &mut iovecs) {
                Ok(n) => break n,
                Err(nix::Error::Sys(ref e))
                    if e == &nix::errno::Errno::EINTR || e == &nix::errno::Errno::EAGAIN => {}
                Err(nix::Error::Sys(e)) => return Err(e.into()),
                _ => unreachable!(),
            }
        };

        assert!(bytes_read > 0);
        let tasks_read = bytes_read / std::mem::size_of::<sys::sg_io_hdr>();
        assert!(tasks_read > 0);
        tasks.extend(
            hdrs.into_iter()
                .map(|hdr| Task::from_underlying(*hdr))
                .take(tasks_read),
        );
        Ok(tasks_read)
    }

    ///
    pub fn perform(&self, task: &Task) -> io::Result<()> {
        #[cfg(target_env = "musl")]
        let request = sys::SG_IO as i32;
        #[cfg(not(target_env = "musl"))]
        let request: u64 = sys::SG_IO.into();

        let ret = unsafe { libc::ioctl(self.0.as_raw_fd(), request, &task.0) };
        if ret == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl std::os::unix::io::AsRawFd for Device {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.0.as_raw_fd()
    }
}

#[cfg(feature = "polling")]
impl Evented for Device {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        EventedFd(&self.0.as_raw_fd()).deregister(poll)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sys() {
        assert_eq!(super::sys::SG_IO, 0x2285);
    }

    #[test]
    fn test_task_fields() {
        let mut task = Task::new();
        let x = 42;
        assert_eq!(task.0.interface_id as u8 as char, 'S');
        task.set_usr_ptr(&x as *const i32 as *const std::os::raw::c_void);
        assert_eq!(task.usr_ptr() as *const i32, &x as *const i32);
        assert_eq!(unsafe { *(task.usr_ptr() as *const i32) }, x);
    }

    #[test]
    fn test_cdb() {
        let cdb = [0; 6];
        let mut task = Task::new();
        task.set_cdb(&cdb);
    }
}
