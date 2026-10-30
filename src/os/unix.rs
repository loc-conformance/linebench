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
