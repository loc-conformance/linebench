use std::mem::offset_of;
use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::SystemInformation::RelationProcessorCore;
use windows_sys::Win32::System::SystemInformation::{
    GROUP_AFFINITY, GetLogicalProcessorInformationEx, PROCESSOR_RELATIONSHIP,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessAffinityMask, GetSystemTimes, SetProcessAffinityMask,
};
use windows_sys::Win32::UI::Shell::IsUserAnAdmin;

use crate::os::{Footprint, PhysicalCore};

const HEADER_BYTES: usize = 8;

pub fn is_user_an_admin() -> bool {
    // Sound because IsUserAnAdmin takes nothing and reads only the calling process's own token.
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn read_process_memory(child: &Child) -> Option<Footprint> {
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
    Some(Footprint {
        resident: counters.WorkingSetSize as u64,
        peak: counters.PeakWorkingSetSize as u64,
        faults: u64::from(counters.PageFaultCount),
        from_disk: 0,
    })
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

pub fn read_core_topology() -> Result<Vec<PhysicalCore>, String> {
    let process_mask = read_affinity_mask()?;
    let mut length: u32 = 0;
    // Sound because a null buffer with a live length pointer asks only for the required byte count.
    unsafe {
        GetLogicalProcessorInformationEx(RelationProcessorCore, std::ptr::null_mut(), &mut length)
    };
    if length == 0 {
        return Err("the processor topology could not be sized".to_string());
    }
    let mut words = vec![0u64; (length as usize).div_ceil(size_of::<u64>())];
    // Sound because words holds at least length bytes aligned to 8 bytes, matching the struct.
    let written = unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            words.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if written == 0 {
        return Err("the processor topology could not be read".to_string());
    }
    let bytes: Vec<u8> = words.into_iter().flat_map(u64::to_ne_bytes).collect();
    let cores = parse_processor_cores(&bytes[..(length as usize).min(bytes.len())], process_mask);
    if cores.is_empty() {
        return Err("no physical cores were found in the process affinity mask".to_string());
    }
    Ok(cores)
}

pub fn read_affinity_mask() -> Result<usize, String> {
    // Sound because GetCurrentProcess takes nothing and returns the calling process's pseudo-handle.
    let process = unsafe { GetCurrentProcess() };
    let mut process_mask: usize = 0;
    let mut system_mask: usize = 0;
    // Sound because both pointers are to live locals of the type the call writes.
    let ok = unsafe { GetProcessAffinityMask(process, &mut process_mask, &mut system_mask) };
    if ok == 0 || process_mask == 0 {
        return Err("the process affinity mask could not be read".to_string());
    }
    Ok(process_mask)
}

pub fn write_affinity_mask(mask: usize) -> Result<(), String> {
    if mask == 0 {
        return Err("an empty process affinity mask cannot be set".to_string());
    }
    // Sound because GetCurrentProcess takes nothing and returns the calling process's pseudo-handle.
    let process = unsafe { GetCurrentProcess() };
    // Sound because process is the calling process's pseudo-handle and mask is a value.
    let ok = unsafe { SetProcessAffinityMask(process, mask) };
    if ok == 0 {
        return Err("the process affinity mask could not be set".to_string());
    }
    Ok(())
}

pub fn build_affinity_mask(cpus: &[usize]) -> Result<usize, String> {
    let mut mask: usize = 0;
    for &cpu in cpus {
        if cpu >= usize::BITS as usize {
            return Err(format!(
                "cpu {cpu} is outside the 64-bit processor group mask"
            ));
        }
        mask |= 1usize << cpu;
    }
    if mask == 0 {
        return Err("an empty cpu list has no affinity mask".to_string());
    }
    Ok(mask)
}

fn parse_processor_cores(bytes: &[u8], process_mask: usize) -> Vec<PhysicalCore> {
    let base = offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Anonymous);
    let eff_off = base + offset_of!(PROCESSOR_RELATIONSHIP, EfficiencyClass);
    let count_off = base + offset_of!(PROCESSOR_RELATIONSHIP, GroupCount);
    let group_base = base + offset_of!(PROCESSOR_RELATIONSHIP, GroupMask);
    let mask_off = group_base + offset_of!(GROUP_AFFINITY, Mask);
    let group_off = group_base + offset_of!(GROUP_AFFINITY, Group);
    let min_record = (mask_off + size_of::<usize>()).max(group_off + size_of::<u16>());
    let mut cores = Vec::new();
    let mut offset = 0usize;
    while offset + HEADER_BYTES <= bytes.len() {
        let Some(rel_bytes) = bytes.get(offset..offset + 4) else {
            break;
        };
        let Some(size_bytes) = bytes.get(offset + 4..offset + 8) else {
            break;
        };
        let relationship =
            i32::from_ne_bytes([rel_bytes[0], rel_bytes[1], rel_bytes[2], rel_bytes[3]]);
        let size = u32::from_ne_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]])
            as usize;
        if size < HEADER_BYTES || offset + size > bytes.len() {
            break;
        }
        let record = &bytes[offset..offset + size];
        if relationship == RelationProcessorCore && record.len() >= min_record {
            let efficiency_class = u32::from(record[eff_off]);
            let group_count = u16::from_ne_bytes([record[count_off], record[count_off + 1]]);
            let group = u16::from_ne_bytes([record[group_off], record[group_off + 1]]);
            let mut mask_bytes = [0u8; size_of::<usize>()];
            mask_bytes.copy_from_slice(&record[mask_off..mask_off + size_of::<usize>()]);
            let mask = usize::from_ne_bytes(mask_bytes) & process_mask;
            if group_count == 1 && group == 0 && mask != 0 {
                let logical_cpus: Vec<usize> = (0..usize::BITS as usize)
                    .filter(|bit| (mask & (1usize << bit)) != 0)
                    .collect();
                cores.push(PhysicalCore {
                    efficiency_class,
                    logical_cpus,
                });
            }
        }
        offset += size;
    }
    cores
}

fn as_ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}
