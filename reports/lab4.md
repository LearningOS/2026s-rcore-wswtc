# CHAPTER 6 实验报告

## 编程作业
### 迁移维护前面章节的系统调用（略）
### 实现sys_linkat
> 思路：查找到old name的目录项，取出起指向的文件inode id，创建一个新的目录项，填入new name，并将其指向文件inode id
>
> linkat需要操作根目录，并且需要比较细粒度的更改data block的内容，因此具体实现的位置应该是在vfs层

本次实现采用“`syscall` 入口做参数处理，`fs` 层做语义实现，`easy-fs` 层做目录项操作”的分层方式。

- 在`os/src/syscall/fs.rs`中实现`sys_linkat`：
  - 使用`current_user_token + translated_str`将用户态字符串参数转换为内核可用的`&str`；
  - 做一些简单的检查；
  - 调用`fs`模块导出的`linkat`完成具体操作。
- 在`os/src/fs/inode.rs`中新增桥接函数：
  - `pub fn linkat(old_name: &str, new_name: &str)`；
  - 直接委托给`ROOT_INODE.linkat(...)`。
- 在`easy-fs/src/vfs.rs`中实现目录项追加：
  - 复用`find_inode_id(old_name, root_disk_inode)`方法找到旧文件inode id；
  - 以目录当前`size`作为插入到位置，确保push到目录的尾部；
  - 构造`DirEntry::new(new_name, old_inode_id)`并写入目录文件尾部，形成硬链接。

### 实现sys_unlinkat
> 思路：删除对应的目录项，如果发现链接到该文件的目录项数目为0，则需要删除文件，具体实现层也在vfs

- 在`os/src/syscall/fs.rs`中实现`sys_unlinkat`：
  - 用户参数字符串转换后直接调用`fs::unlinkat`。
- 在`os/src/fs/inode.rs`中新增桥接函数：
  - `pub fn unlinkat(name: &str)` -> `ROOT_INODE.unlinkat(name)`。
- 在`easy-fs/src/vfs.rs`中实现`Inode::unlinkat`：
  1. 先通过名字找到目标inode id；
  2. 遍历根目录统计指向该inode的目录项个数`cnt`，并记录待删除目录项下标`file_index`；
  3. 若`cnt > 1`：只把该目录项覆盖为 DirEntry::empty()`，即只删除这一个名字；
  4. 若`cnt == 1`：先定位inode并`clear()`清空文件数据，再将目录项置空。

> 感觉read_disk_inode和modify_disk_inode实现的特别丑，完全没必要传闭包进去。
> 遇到了一个坑： 
> 如果在持有`self.fs`锁时再调用`write_at`（而`write_at`内部也会加同一把锁），会出现卡住现象。后续通过缩小锁作用域（先取位置，释放锁，再执行后续写操作）避免了这个问题。

### 实现sys_fstat

> 思路：这个系统调用需要获取File对象的状态，也即只要实现了file trait的类型都应该实现这个系统调用，不仅包括block device，还包括stdin和stdout，因此实现的层数应该在trait层

- 在`os/src/fs/mod.rs`扩展`File` trait
- 在`os/src/fs/stdio.rs`中为 `Stdin/Stdout` 补齐该接口（题目没说清楚，因此默认不会获取stdin和stdout的状态，当前实现为`panic!`）。
- 在`os/src/fs/inode.rs`中为 `OSInode`实现`stat()`：
  - `ino`：通过inode的磁盘位置反推inode id（复用`find_inode_id`）；
  - `mode`：通过`is_dir/is_file`映射到`StatMode::{DIR, FILE}`；
  - `nlink`：遍历根目录目录项，统计inode id相同的条目数（`ROOT_INODE.get_nlink(ino)`）；
  - 其他字段置零
- 在`os/src/syscall/fs.rs`中实现`sys_fstat`：
  - 通过 `translated_refmut` 获取用户态 `Stat` 写回地址；
  - 从当前进程`fd_table`取出文件对象，调用`inode.stat()`写回。

### 第六章问答作业
### 在我们的easy-fs中，root inode起着什么作用？如果root inode中的内容损坏了，会发生什么？

`root inode`是easy-fs的目录树入口，也是当前实验实现中绝大多数文件操作（`open/find/create/link/unlink/ls`）的起点。它的作用可以概括为：

1. **命名空间入口**：保存根目录下所有目录项（文件名 -> inode id 映射）；
2. **路径解析起点**：当前实验只处理根目录文件名，所有查找都从它开始；
3. **链接计数统计依据**：`nlink` 的计算依赖遍历根目录目录项；
4. **文件创建挂载点**：新文件的目录项要写入 root inode 对应目录数据区。

如果root inode内容损坏：无法获取文件系统里真实的文件信息，但笔者认为可以修复。在已知文件系统的情况下，可以通过遍历每个block的方法识别所有正常的文件，即可重建root inode。

## 第七章问答作业
### 举出使用 pipe 的一个实际应用的例子。
> tips:
> 想想你平时咋使用 linux terminal 的？
> 如何使用 cat 和 wc 完成一个文件的行数统计？

`cat file.txt | wc -l`

这个命令用于统计`file.txt`的行数。

其工作过程是：

1. `cat file.txt` 读取文件内容，并把内容输出到标准输出；
2. shell 创建一条 `pipe`；
3. `cat` 进程的标准输出被重定向到管道写端；
4. `wc -l` 进程的标准输入被重定向到管道读端；
5. `wc -l` 从管道中持续读取数据，并统计其中的换行符个数，最后输出总行数。

### 如果需要在多个进程间互相通信，则需要为每一对进程建立一个管道，非常繁琐，请设计一个更易用的多进程通信机制。
1. 在内核中实现一个“message管理器”
2. 每个进程自己都有一个mailbox，每个mailbox分为发送缓冲区和接收缓冲区
3. 当a进程要发送信息到b进程时，a讲内容送到自己mailbox的发送缓冲区，经由内核中的message管理器讲信息送到b的mailbox接收缓冲区中，完成进程间的通信

以上是我个人框架性的想法，接下来交给AI发挥扩充：

我认为可以将其进一步设计为一种基于 **mailbox（邮箱）/ message queue（消息队列）** 的 IPC 机制。相比于 `pipe` 必须为每一对通信进程单独建立通道，这种机制只需要为每个进程维护一个固定的通信端点，使用起来会自然很多。

具体设计如下：

4. **每个进程只保留一个核心 mailbox 即可**
   - 实际上 mailbox 更适合作为“收件箱”，用于保存其他进程发给自己的消息；
   - 不一定必须显式维护“发送缓冲区”，因为发送动作本质上只是一次系统调用，消息可以直接由内核投递到目标进程的 mailbox 中；
   - 每个 mailbox 内部可以实现为一个循环队列或 `VecDeque`，队列中的每一项都是一条消息。

5. **消息的数据结构**
   - 每条消息至少应包含：
     - 发送方 pid
     - 接收方 pid
     - 消息长度
     - 消息内容
     - 可选的消息类型/优先级
   - 这样接收方不仅能读到数据，还能知道是谁发送的，以及该如何解释这条消息。

6. **内核中的 message 管理器**
   - 内核维护一个全局的 message manager，负责：
     - 根据 pid 找到目标进程的 mailbox；
     - 完成消息投递；
     - 检查 mailbox 是否已满；
     - 在进程等待消息时负责阻塞与唤醒；
     - 做必要的权限检查与资源回收。
   - 从职责上讲，它相当于整个消息通信机制的调度与分发中心。

7. **发送消息的过程**
   - 当进程 A 调用 `send(pid_b, msg)` 时：
     1. 触发系统调用进入内核；
     2. 内核检查目标进程 B 是否存在；
     3. 若存在，则找到 B 的 mailbox；
     4. 将消息封装后放入 B 的 mailbox 队尾；
     5. 若 B 此时正在等待接收消息，则立即将其唤醒。
   - 这样 A 不需要知道 B 是否提前建立了某条“专用通道”，只需要知道 B 的 pid 即可。

8. **接收消息的过程**
   - 当进程 B 调用 `recv()` 时：
     - 如果自己的 mailbox 非空，就取出队首消息并返回；
     - 如果 mailbox 为空，则有两种可选策略：
       - 阻塞等待，直到有新消息到达；
       - 立即返回错误码，交给用户态决定是否轮询。
   - 为了提高易用性，我认为阻塞式接收更适合作为默认语义。

9. **容量与同步问题**
   - 若 mailbox 已满，则 `send()` 也有多种策略：
     - 阻塞等待空间；
     - 返回错误码；
     - 丢弃旧消息或新消息。
   - 实现时需要配合锁、等待队列以及进程调度机制，保证多个发送者同时向同一目标发送消息时不会破坏队列一致性。

10. **相对于 pipe 的优势**
    - `pipe` 更适合两个进程之间的字节流通信；
    - mailbox/message queue 更适合多进程、按目标寻址的离散消息通信；
    - 每个进程只需维护一个通信入口，不需要为每对进程建立一条管道；
    - 更容易扩展出“一个进程接收多个进程消息”“按消息类型分类处理”“支持优先级消息”等高级功能。

总结来说，这种机制的核心思想是：  
**由内核统一维护消息投递，每个进程只暴露一个 mailbox 作为固定通信端点，发送方通过 pid 寻址，接收方从自己的 mailbox 中取消息。**  
这样既减少了 `pipe` 在多进程场景下的连接复杂度，也更符合“消息传递型 IPC” 的使用习惯。
