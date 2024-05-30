use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    if mutex.lock() {
        0
    } else {
        -0xDEAD
    }
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    let task = current_task();
    let mut task_inner = task.as_ref().unwrap().inner_exclusive_access();
    info!("actively realse sem {}", sem_id);
    if let Some(index) = task_inner.allocation.iter().enumerate().find(|(_,(id,_))| *id == sem_id).map(|(index,(_,_))| index) {
        task_inner.allocation[index].1 -= 1;
        if task_inner.allocation[index].1 == 0 {
            task_inner.allocation.remove(index);
        }
    }
    sem.up(sem_id);
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    // the banker's algorithm, first, get current task's id, be used to judge whether is the current task
    let cur_task = current_task();
    let cur_task_inner = cur_task.as_ref().unwrap().inner_exclusive_access();
    let cur_tid = cur_task_inner.res.as_ref().unwrap().tid;
    drop(cur_task_inner);
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    if process_inner.need_dead_lock_detect {
        // initialize the params of this algogrithm
        let (thread_count, sem_count) = (process_inner.tasks.len(), process_inner.semaphore_list.len());
        let mut finished = vec![false; thread_count];                        // vector `finish`
        let mut work = vec![0; sem_count];                                  // vector `work`
        let mut allocation = vec![vec![0; sem_count]; thread_count];   // matrix `allocation`
        let mut need = vec![vec![0; sem_count]; thread_count];         // matrix `need`
        for (index, task) in process_inner.tasks.iter().enumerate() {
            let task_inner = task.as_ref().unwrap().inner_exclusive_access();
            let tid = task_inner.res.as_ref().unwrap().tid;
            for (id,cnt) in task_inner.allocation.iter() {
                allocation[index][*id] = *cnt;
            }
            for (id,cnt) in task_inner.need.iter() {
                need[index][*id] = *cnt;
            }
            if tid == cur_tid {
                // if tid stands for current task, it need to add one, for the 'banker' to judge whether still safe
                need[index][sem_id] += 1;
            }
        }
        for (index,sem) in process_inner.semaphore_list.iter().enumerate() {
            work[index] = sem.as_ref().unwrap().inner.exclusive_access().count.max(0);
        }
        
        for k in 0..thread_count {
            for i in 0..thread_count {
                if finished[i] == true {
                    // finished tasks
                    continue;
                }
                let mut flag = true;
                for j in 0..sem_count{
                    if need[i][j] > work[j] { // can't be execute
                        flag = false;
                    }
                }
                if flag {
                    finished[i] = flag;
                    for j in 0..sem_count { // get resource back
                        work[j] += allocation[i][j];
                    }
                    break; // find next
                }
            }
            debug!("round{}, finished:{:?}, work:{:?}", k, finished, work);
        }
        debug!("banker-final >> finished:{:?}, work:{:?}", finished, work);
        if finished.iter().any(|value| *value == false) {
            return -0xDEAD;
        }
    }
    drop(process_inner);
    if sem.inner.exclusive_access().count <= 0 {
        let task = current_task();
        let mut task_inner = task.as_ref().unwrap().inner_exclusive_access();
        if let Some(index) = task_inner.need.iter().enumerate()
        .find(|(_,(id,_))| *id == sem_id)
        .map(|(index,(_,_))| index) {
            task_inner.need[index].1 += 1;
        } else {
            task_inner.need.push((sem_id,1));
        }
    } else {
        let task = current_task();
        let mut task_inner = task.as_ref().unwrap().inner_exclusive_access();
        if let Some(index) = task_inner.allocation.iter().enumerate()
        .find(|(_,(id,_))| *id == sem_id)
        .map(|(index,(_,_))| index) {
            task_inner.allocation[index].1 += 1;
        } else {
            task_inner.allocation.push((sem_id,1));
        }
    }
    sem.down();
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect");
    match enabled {
        1 => current_process().inner_exclusive_access().need_dead_lock_detect = true,
        0 => current_process().inner_exclusive_access().need_dead_lock_detect = false,
        _ => return -1,
    };
    0
}
