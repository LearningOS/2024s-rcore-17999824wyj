//! Semaphore

use crate::sync::UPSafeCell;
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self, sem_id: usize) {
        trace!("kernel: Semaphore::up");
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                // when someone of semaphore is freed, it can be used by others
                // but should notice, it may not be in 'allocation' before, so we just need to add an item to handle this situation 
                let mut task_inner = task.inner_exclusive_access();
                if let Some(index) = task_inner.need.iter().enumerate()
                .find(|(_,(id,_))| *id == sem_id)
                .map(|(index,(_,_))| index) {
                    task_inner.need[index].1 -= 1;
                }
                if let Some(index) = task_inner.allocation.iter().enumerate()
                .find(|(_,(id,_))| *id == sem_id)
                .map(|(index,(_,_))| index) {
                    task_inner.allocation[index].1 += 1;
                } else {
                    task_inner.allocation.push((sem_id,1));
                }
                drop(task_inner);
                wakeup_task(task);
            }
        }
    }

    /// down operation of semaphore
    pub fn down(&self) {
        trace!("kernel: Semaphore::down");
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        }
    }
}
