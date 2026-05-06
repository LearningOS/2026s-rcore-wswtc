# CHAPTER 6 实验报告

## 编程作业

> [!NOTE]
> 大致完成死锁检测的所有代码用时：3h
> 调试死锁检测最终识别到所有的坑并通过测试用例用时：8h

### 维护sys_get_time
> 这里遇到了一个大坑，没仔细看源码以为get_time()接口获取的是秒，所以一开始直接用的sec = get_time()。实际上这个结构返回的是时钟周期
> 导致即使后面正确地实现了银行家算法，也无法通过测试用例`ch8_deadlock_sem1`
> 原因在于：该测试用例中主线程创建完三个子线程准备模拟死锁环境后，使用`sleep(500)`进行阻塞。
> 由于sys_get_time()实现错误，导致只阻塞了500个时钟周期就释放了资源,从而无法模拟死锁环境，最终导致测试用例失败。

### 实现银行家算法
> 实现步骤分四步走：
> 1. 根据银行家算法的原理，构思需要的数据结构：available数组, need表, allocation表，这三类数据结构需要全局线程可见，因此放置位置在`ProcessControlBlockInner`中
> 2. 如何初始化以上数据结构：在新建线程时，为need表和allocation表插入新行；在新建mutex或者semaphore时，为available表插入新元素，并为need和allocation表插入新列
> 3. 如何维护以上数据结构：在通过死锁检测后且在lock/down成功前，对应need加一；lock/down成功后对应need减一，available减一，allocation加一;unlock/up成功后available加一，allocation减一
> 4. 万事俱备，只欠真正实现银行家算法：本实现中把死锁检测视为PCB的一项功能，因此实现在task/process.rs中

- 在`task/process.rs`中实现死锁检测方法：`pub fn is_deadlock(inner: &RefMut<'_, ProcessControlBlockInner>, cur_tid: usize, req_id: usize, req_amount: usize,) -> bool`
  - `work`数组和临时`need`表在方法中clone形成
  - 死锁检测算法当中的所有资源分配均为模拟，并不会对真正的全局数据结构产生影响
- 算法本体只占非常小的一部分，绝大部分工作都在维护全局数据结构
  - 在`task/prcess.rs`的new()、exec()、for()方法中为主线程初始化全局数据结构
  - 在`syscall/hread.rs`中为新建线程添加全局的表信息
  - 在`syscall/syn.rs`中维护全局数据结构的真实变化
  - 本实现统一了mutex和semaphore的死锁检测，也即可以适用于mutex和semaphore一起作用产生的死锁场景
- 全局数据结构的真实面目：

```rust
    pub available: BTreeMap<usize, usize>,
    pub need: BTreeMap<usize, BTreeMap<usize, usize>>,
    pub allocation: BTreeMap<usize, BTreeMap<usize, usize>>,
```

其实按照性能来说应该使用Vec<Option<_>>，但由于rcore的实现中没有实现全局的资源分配器，导致无法知道某一资源编号是否存在，也无法区分统一编号下的mutex和semaphore
因此为了减轻认知负担，本实现使用BTreeMap来映射资源id及其对应的数量，同时为了应对mutex和semaphore同编号冲突的问题，在全局数据结构中semaphore的编号统一增加250：

```rust
process_inner.available.insert(id + 250, res_count);
process_inner.allocation.values_mut().for_each(|v| {
    v.insert(id + 250, 0);
});
process_inner.need.values_mut().for_each(|v| {
    v.insert(id + 250, 0);
});
```

### 实现系统调用sys_enable_deadlock_detect

增加一个标志位即可

## 问答作业

### 在我们的多线程实现中，当主线程 (即 0 号线程) 退出时，视为整个进程退出， 此时需要结束该进程管理的所有线程并回收其资源。 - 需要回收的资源有哪些？ - 其他线程的 TaskControlBlock 可能在哪些位置被引用，分别是否需要回收，为什么？

需要回收的资源（对应到PCB结构体）：
1. 地址空间（页表、及其映射的物理页）
2. 所有子进程
3. 文件描述符
4. 所有线程
5. 所有mutex和semaphore以及condvar
6. 与死锁检测有关的数据结构(available、need、allocation)

其他线程的TaskControlBlock可能被以下位置引用：
1. 调度器manager的ready_queue和stop_task：需要回收，当主线程结束时，一些子线程可能还在ready_queue和stop_task中，因此需要强行把子线程从ready_queue和stop_task中移除并回收资源
2. 处理器管理processor的current：需要回收，当主线程结束时，一些子线程可能还在processor的current中，因此需要强行把子线程从processor的current中移除并回收资源
3. Timer中的TimerCondvar：需要回收，当主线程结束时，一些子线程可能还在TimerCondvar的wait_queue中，同上
4. condvar、mutex和semaphore当中的wait_queue：需要回收，理由同上

### 对比以下两种 Mutex 中的实现，二者有什么区别？这些区别可能会导致什么问题？

```rust
impl Mutex for Mutex1 {
    fn lock(&self) {
        loop {
            let mut mutex_inner = self.inner.exclusive_access();
            if mutex_inner.locked {
                mutex_inner.wait_queue.push_back(current_task().unwrap());
                drop(mutex_inner);
                block_current_and_run_next();
            } else {
                mutex_inner.locked = true;
                break;
            }
        }
    }

    fn unlock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        mutex_inner.locked = false;
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            add_task(waking_task);
        }
    }
}

impl Mutex for Mutex2 {
    fn lock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            mutex_inner.locked = true;
        }
    }

    fn unlock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            add_task(waking_task);
        } else {
            mutex_inner.locked = false;
        }
    }
}
```

**Mutex1**是**“Mesa语义”（信号量式竞争）**，**Mutex2**是**“直接移交（Direct Handoff）”语义**。

| 特性 | Mutex1 (Mesa 风格) | Mutex2 (直接移交风格) |
| :--- | :--- | :--- |
| **唤醒逻辑** | `unlock` 时先将 `locked` 设为 `false`，再唤醒等待者。 | `unlock` 时如果不为空，**保持** `locked = true`，直接把控制权交给等待者。 |
| **等待逻辑** | `lock` 使用 `loop`。被唤醒后需要重新检查 `locked` 状态。 | `lock` 使用 `if`。被唤醒后**默认**自己已经持有了锁，直接退出。 |
| **锁的归属** | 锁被释放回“自由市场”，唤醒的线程需与新来的线程竞争。 | 锁在内核中完成“接力”，直接从当前线程移交给队列首部线程。 |

#### **Mutex1的问题：潜在的“不公平”与“惊群”**
* **锁插队（Barging）：** 当 `unlock` 把 `locked` 设为 `false` 并唤醒线程 A 时，如果此时正好有一个新线程 B 调用 `lock`，B 可能会在 A 真正跑起来之前抢先发现 `locked == false` 并上锁。
* **后果：** 被唤醒的线程 A 重新进入 `loop` 发现锁又被占了，被迫再次阻塞。这可能导致 **线程饥饿**，即等待时间最久的线程反而抢不到锁。

#### **Mutex2 的问题：逻辑脆弱性与死锁风险**

1.  **假设失效（健壮性差）：**
    `Mutex2` 的 `lock` 函数假设：**只要我被唤醒，我就一定拿到了锁。**
    如果操作系统支持信号（Signal）或存在“虚假唤醒”（Spurious Wakeup），线程可能从 `block_current_and_run_next()` 返回，却并不是因为 `unlock` 唤醒的。此时它会带着 `locked = true` 错误地进入临界区，导致多个线程同时访问资源。

2.  **调度器强耦合：**
    在 `Mutex2` 的 `unlock` 中，锁的 `locked` 状态依赖于 `add_task(waking_task)` 的成功。如果 `add_task` 失败或该线程中途被销毁（Kill），锁将永远处于 `locked = true` 状态，即使等待队列已经空了，后续线程也无法再获取锁，从而导致 **永久性死锁**。

3.  **所有权逻辑模糊：**
    在 Rust 的语义下，Mutex 通常需要保证上锁和解锁是配对的。`Mutex2` 的实现中，锁的状态位由一个线程设置，却由另一个线程“隐式继承”，如果涉及复杂的递归锁或优先级继承，这种隐式移交会导致内核记账逻辑极其混乱。

经过使用AI的学习，建议使用第一种mesa语义，配合公平队列既可以避免饥饿问题。
