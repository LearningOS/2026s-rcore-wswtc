//! Process management syscalls
use crate::mm::{is_frame_available, MapPermission, PageTable, PhysAddr, VirtAddr};
use crate::task::{
    change_program_brk, current_memoryset_insert_framed_area, current_memoryset_unmap_range,
    current_user_token, exit_current_and_run_next, get_syscalls_count,
    suspend_current_and_run_next,
};
use crate::timer::get_time_us;
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let user_pagetable = PageTable::from_token(current_user_token());
    let sec_va = VirtAddr::from(ts as usize);
    let usec_va = VirtAddr::from(ts as usize + core::mem::size_of::<usize>());
    //简单粗暴试一下
    let sec_ppn = user_pagetable.translate(sec_va.floor()).unwrap().ppn();
    let usec_ppn = user_pagetable.translate(usec_va.floor()).unwrap().ppn();
    let sec_pa = PhysAddr::from(sec_ppn).0 + sec_va.page_offset();
    let usec_pa = PhysAddr::from(usec_ppn).0 + usec_va.page_offset();
    let us = get_time_us();
    unsafe {
        *(sec_pa as *mut usize) = us / 1_000_000;
        *(usec_pa as *mut usize) = us % 1_000_000;
    }
    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => {
            let user_pagetable = PageTable::from_token(current_user_token());
            let va = VirtAddr::from(id);
            let pte = user_pagetable.translate(va.floor());
            if !pte.is_some_and(|x| x.is_valid() && x.readable() && x.is_user_accessible()) {
                return -1;
            }
            pte.unwrap().ppn().get_bytes_array()[va.page_offset()] as isize
        }
        1 => {
            let user_pagetable = PageTable::from_token(current_user_token());
            let va = VirtAddr::from(id);
            let pte = user_pagetable.translate(va.floor());
            if !pte.is_some_and(|x| x.is_valid() && x.writable() && x.is_user_accessible()) {
                return -1;
            }
            pte.unwrap().ppn().get_bytes_array()[va.page_offset()] = data as u8;

            0
        }
        2 => get_syscalls_count(id) as isize,
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    let start_va = VirtAddr::from(start);
    // validate prot bits: only low 3 bits valid and at least one must be set
    // and start must be page aligned and physical memory must be available
    if prot & !0x7 != 0 || prot & 0x7 == 0 || !start_va.aligned() || !is_frame_available() {
        return -1;
    }
    // len == 0: nothing to map, treat as success
    if len == 0 {
        return 0;
    }
    // compute end VA by rounding up len to pages
    let end_addr = start + len;
    let end_va = VirtAddr::from(end_addr);
    // check for existing mappings in range [start, end) using current user page table
    let user_pt = PageTable::from_token(current_user_token());
    let mut vpn = start_va.floor();
    let end_vpn = end_va.ceil();
    while vpn < end_vpn {
        let pte = user_pt.translate(vpn);
        if pte.is_some() && pte.unwrap().is_valid() {
            return -1;
        }
        vpn.0 += 1;
    }
    // build MapPermission from prot bits and include U bit
    let perm = MapPermission::U | MapPermission::from_bits((prot as u8) << 1).unwrap();
    // perform mapping (allocates frames). We assume insert_framed_area will panic on OOM
    current_memoryset_insert_framed_area(start_va, end_va, perm);
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if len == 0 {
        return 0;
    }
    let start_va = VirtAddr::from(start);
    if !start_va.aligned() {
        return -1;
    }

    let end_va = VirtAddr::from(start + len);

    // check that all pages in [start, end) are mapped using page table
    let user_pt = PageTable::from_token(current_user_token());
    let mut vpn = start_va.floor();
    let end_vpn = end_va.ceil();
    while vpn < end_vpn {
        let pte = user_pt.translate(vpn);
        if pte.is_none() || !pte.unwrap().is_valid() {
            return -1;
        }
        vpn.0 += 1;
    }
    // delegate removal/shrink to task API: we only support removing areas that start at `start`
    if current_memoryset_unmap_range(start_va, end_va) {
        return 0;
    }
    // If shrink_to returned false, we don't support arbitrary subrange removal -> error
    -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
