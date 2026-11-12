use std::io;
#[cfg(target_os = "linux")]
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use crate::os::Usage;

/// The kernel counts `ru_inblock` in 512-byte blocks, whatever the block size of the device.
#[cfg(target_os = "linux")]
const INBLOCK_BYTES: u64 = 512;

#[cfg(target_os = "linux")]
pub fn read_affinity_cpus() -> Result<Vec<usize>, String> {
    // Sound because cpu_set_t is a plain bitmask array with no invalid all-zero representation.
    let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    // Sound because the pointer is to a live cpu_set_t local and size_of gives its exact byte size.
    let status = unsafe { libc::sched_getaffinity(0, size_of::<libc::cpu_set_t>(), &mut set) };
    if status != 0 {
        return Err("the process affinity mask could not be read".to_string());
    }
    let mut cpus = Vec::new();
    for cpu in 0..libc::CPU_SETSIZE as usize {
        // Sound because cpu is strictly below CPU_SETSIZE and set is an initialized cpu_set_t.
        let present = unsafe { libc::CPU_ISSET(cpu, &set) };
        if present {
            cpus.push(cpu);
        }
    }
    if cpus.is_empty() {
        return Err("the process affinity mask is empty".to_string());
    }
    Ok(cpus)
}

#[cfg(not(target_os = "linux"))]
pub fn read_affinity_cpus() -> Result<Vec<usize>, String> {
    Err("cpu affinity is not supported on this platform".to_string())
}

#[cfg(target_os = "linux")]
pub fn write_affinity_cpus(cpus: &[usize]) -> Result<(), String> {
    if cpus.is_empty() {
        return Err("an empty process affinity mask cannot be set".to_string());
    }
    // Sound because cpu_set_t is a plain bitmask array with no invalid all-zero representation.
    let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    for &cpu in cpus {
        if cpu >= libc::CPU_SETSIZE as usize {
            return Err(format!("cpu {cpu} is outside CPU_SETSIZE"));
        }
        // Sound because cpu is strictly below CPU_SETSIZE and set is a live local cpu_set_t.
        unsafe { libc::CPU_SET(cpu, &mut set) };
    }
    // Sound because the pointer is to a live cpu_set_t local and size_of gives its exact byte size.
    let status = unsafe { libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), &set) };
    if status != 0 {
        return Err("the process affinity mask could not be set".to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn write_affinity_cpus(_cpus: &[usize]) -> Result<(), String> {
    Err("cpu affinity is not supported on this platform".to_string())
}

#[cfg(target_os = "linux")]
pub fn reap_without_blocking(pid: u32) -> io::Result<Option<(ExitStatus, Usage)>> {
    let mut status = 0;
    // Sound because rusage is plain integers and timevals with no invalid all-zero representation.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // Sound because both pointers are to live locals of the exact types wait4 writes.
    let reaped = unsafe { libc::wait4(pid as libc::pid_t, &mut status, libc::WNOHANG, &mut usage) };
    match reaped {
        0 => Ok(None),
        -1 => Err(io::Error::last_os_error()),
        _ => Ok(Some((
            ExitStatus::from_raw(status),
            Usage {
                faults: usage.ru_minflt as u64,
                disk_read_bytes: usage.ru_inblock as u64 * INBLOCK_BYTES,
            },
        ))),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn reap_without_blocking(_pid: u32) -> io::Result<Option<(ExitStatus, Usage)>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "a run is reaped with its resource usage on linux",
    ))
}

#[cfg(all(test, target_os = "linux"))]
pub fn drop_from_page_cache(file: &std::fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // Sound because the descriptor belongs to a live File, and a length of zero means all of it.
    let failed = unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
    match failed {
        0 => Ok(()),
        code => Err(io::Error::from_raw_os_error(code)),
    }
}
