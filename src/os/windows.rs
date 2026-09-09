#![allow(unsafe_code)]

use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::ProcessStatus::GetProcessMemoryInfo;
use windows_sys::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS;
use windows_sys::Win32::System::Threading::GetSystemTimes;
use windows_sys::Win32::UI::Shell::IsUserAnAdmin;

pub fn is_user_an_admin() -> bool {
    // Sound because IsUserAnAdmin takes nothing and reads only the calling process's own token.
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn read_process_memory(child: &Child) -> Option<(u64, u64)> {
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // Sound because the pointer is to a live local and cb says how much of it the call may write.
    let written =
        unsafe { GetProcessMemoryInfo(child.as_raw_handle(), &mut counters, counters.cb) };
    if written == 0 {
        return None;
    }
    Some((
        counters.WorkingSetSize as u64,
        counters.PeakWorkingSetSize as u64,
    ))
}

pub fn read_system_times() -> Option<(u64, u64)> {
    let mut idle = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut kernel = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut user = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // Sound because the three pointers are to live locals of the type the call writes.
    let written = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    if written == 0 {
        return None;
    }
    let idle = as_ticks(idle);
    Some((idle, as_ticks(kernel) + as_ticks(user)))
}

fn as_ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}
