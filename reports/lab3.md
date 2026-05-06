# CHAPTER 5 实验报告

## 编程作业
### 迁移`sys_get_time`, `sys_mmap`, `sys_munmap`
- `sys_get_time`的实现思路与上一章相同，也即分别对结构体里的每个usize都进行一次虚存到物理内存的转换，无论跨页与否都能正确访问到每个usize的正确物理内存。与上一章节不同的是，复用了rcore提供的接口`translated_refmut`和`current_user_token`，不再需要unsafe块。
- `sys_mmap`的实现与上一章完全相同，但我感觉我这个实现方式有点丑陋，肯定还有什么更优雅的实现
- `sys_munmap`的实现思路也与上一章差不多，区别在于这里我复用了提供的接口`remove_area_with_start_vpn`，上一章我想太多了导致实现的有点复杂，还需要回去看看我当时是怎么想的。

### 实现`sys_spawn`系统调用
> 为了实现spawn，我需要做些什么？（可以参考exec的实现）
> 1. 创建一个新的PCB，复用new方法，直接从零到完整地创建好一个进程
> 2. 设置父子进程的指针
> 3. 将spawn出来的进程加入调度队列
> 4. 完成，返回pid

- 创建pcb时需要传入的path找到app_data，因此需要先后复用以下接口：
  - `translated_str`：将传入的u8指针指向的虚拟内存通过查找到其真实物理内存转换为内核可访问的String类型
  - `get_app_data_by_name`：从string找到对应的app二进制可执行文件
- 新建TCB时，需要用Arc包起来
- Arc::downgrade获取弱引用
- 最后加入调度队列时直接将所有权交出去，不要使用clone

### 实现Stride调度算法
- 为TCB新增两个字段：`pub stride: usize`和`pub pass: usize`
  - 同时在new方法里设置stride的初始值为0，pass的初始值为64，也即每个任务初始时它们的优先级都为16(`STRIDE = 1024`)
  - 在fork方法里设置子进程的stride和pass均继承自父进程
- 真正实现调度算法的地方：`TaskManager::new()`和`run_task()`方法
  - 在`new()`方法中，遍历ready队列，找到stride最小的任务弹出
  - 在`run_task()`中添加一行：`task_inner.stride += task_inner.pass;`即可，我感觉这里也可以在`fetch_task`里加
- 实现`sys_set_priority`系统调用：计算pass = BIG_STRIDE / prio即可，在本实现中BIG_STRIDE = 1024

## 问答作业
> stride 算法原理非常简单，但是有一个比较大的问题。例如两个 pass = 10 的进程，使用 8bit 无符号整形储存 stride，
> p1.stride = 255, p2.stride = 250，在 p2 执行一个时间片后，理论上下一次应该 p1 执行。

### 实际情况是轮到 p1 执行吗？为什么？

实际情况还是p2，选择下一个时间片的任务时，p1.stride = 255, p2.stride = 4（溢出了），因此在调度器看来p2.stride更小，还是轮到p2执行

> 我们之前要求进程优先级 >= 2 其实就是为了解决这个问题。可以证明， 在不考虑溢出的情况下 , 
> 在进程优先级全部 >= 2 的情况下，如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。

### 为什么？尝试简单说明（不要求严格证明）。
在规定了prio >= 2的情况下，pass最大不超过BigStride / 2，因此对于优先级最小的任务来说，stride每次最多只会增加BigStride / 2，其后则轮到其他优先级较高的任务执行，并且这些任务的stride增加都在BigStride / 2范围内。在这种规定下，可以使用模运算的方式处理溢出并保证顺序不会翻转，最应该被执行的任务和最后才需要被执行的任务的距离不会超过环形队列的一半

- priority ≥ 2 保证 stride 不会跨越模空间的一半，因此比较大小不会出错。

```rust
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let diff = self.0.wrapping_sub(other.0);

        if diff < (1 << 63) {
            Some(Ordering::Greater)
        } else {
            Some(Ordering::Less)
        }
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        false
    }
}

```
