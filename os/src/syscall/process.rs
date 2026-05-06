//! Process management syscalls
use alloc::sync::Arc;

use crate::{
    loader::get_app_data_by_name,
    mm::{
        is_frame_available, translated_refmut, translated_str, MapPermission, PageTable, VirtAddr,
    },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskControlBlock,
    },
    timer::{get_time, get_time_us},
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!(
        "kernel::pid[{}] sys_waitpid [{}]",
        current_task().unwrap().pid.0,
        pid
    );
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // 会出现找不到current_task的情况吗？
    let sec_pa = translated_refmut(current_user_token(), ts as *mut usize);
    // 前面的章节没有提供这个接口，实在是太好用辣
    let usec_pa = translated_refmut(current_user_token(), unsafe { (ts as *mut usize).add(1) });
    // 这个接口太牛逼了，直接不用unsafe，上一章的我的想象力还是太贫瘠了
    *sec_pa = get_time();
    *usec_pa = get_time_us();

    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .memory_set
        .insert_framed_area(start_va, end_va, perm);
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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

    let mut start_vpn = start_va.floor();
    while start_vpn < end_vpn {
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .memory_set
            .remove_area_with_start_vpn(start_va.floor());
        start_vpn.0 += 1;
    }
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // 为了实现spawn，我需要做些什么？（可以参考exec的实现）
    // 1. 创建一个新的PCB，调用new方法，直接从零到完整的创建好一个进程
    // 2. 设置父子进程的指针
    // 3. 将spawn出来的进程加入调度队列
    // 4. 完成（忘记返回的是pid了）

    //创建新进程
    let current_user_token = current_user_token();
    let app_path = translated_str(current_user_token, path);
    let app_data = if let Some(data) = get_app_data_by_name(app_path.as_str()) {
        data
    } else {
        return -1;
    };
    let new_task = Arc::new(TaskControlBlock::new(app_data));
    let new_pid = new_task.pid.0 as isize;

    //设置父子指针
    let current_task = current_task().unwrap();
    new_task.inner_exclusive_access().parent = Some(Arc::downgrade(&current_task));
    current_task
        .inner_exclusive_access()
        .children
        .push(new_task.clone());

    // 加入调度队列，这里不用clone，直接把所有权交给调度器
    add_task(new_task);
    new_pid
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if prio < 2 {
        return -1;
    }
    const BIG_STRIDE: usize = 1 << 10;
    current_task().unwrap().inner_exclusive_access().pass = BIG_STRIDE / (prio as usize);

    prio
}
