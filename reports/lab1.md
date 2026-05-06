# CHAPTER 3 实验报告

## 编程作业：实现sys_trace系统调用
> 基本思路：
> 每个任务的系统调用统计信息属于在运行时不断变化的属性，因此适合存储于TCB中
> 扩展TCB结构体，增加用于统计系统调用的数组(相当于一个简单的线性哈希表)
> 在syscall入口函数中接受每次系统调用，然后增加当前任务的系统调用统计信息值即可
1. task/task.rs: 扩展TaskControlBlock结构体，增加字段`pub task_syscalls_count: [usize; 500]`
2. task/mod.rs:
    - 增加函数`count_syscalls(syscall_id: usize) -> usize`，获取当前任务的统计数组，并对相应系统调用值加一
    - 增加函数`get_syscalls_count(syscall_id: usize) -> isize`获取所当前任务所查询的系统调用统计信息，供sys_trace使用
    - 在TASK_MANAGER运行时初始化中添加对应task_syscalls_count的初始化代码
2. syscall/mod.rs: 调用`count_syscalls(syscall_id)`
3. syscall/process.rs: 调用`get_syscalls_count(id)`
4. 在sys_trace函数中继续完成另外两个简单的trace_request需求


## 简答题

### 1. 正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。请同学们可以自行测试这些内容（运行 三个 bad 测例 (ch2b_bad_*.rs) ）， 描述程序出错行为，同时注意注明你使用的 sbi 及其版本。
> RustSBI版本：RustSBI-QEMU Version 0.2.0-alpha.2
1. ch2b_bad_address.rs:
    - 错误信息：[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
    - 错误行为：访问非法地址0x0，触发StoreFault异常(也有可能是StorePageFault？)，在trap_handler函数中打印异常信息并杀掉任务
2. ch2b_bad_instructions.rs
    - 错误信息：[kernel] IllegalInstruction in application, kernel killed it.
    - 错误行为：处在U特权级的应用使用S特权级的指令，触发IllegalInstruction异常，在trap_handler函数中打印异常信息并杀掉任务
3. ch2b_bad_register.rs
    - 错误信息：[kernel] IllegalInstruction in application, kernel killed it.
    - 错误行为：处在U特权级的应用访问只有S特权级才可见的寄存器，触发IllegalInstruction异常，在trap_handler函数中打印异常信息并杀掉任务

### 2. 深入理解 trap.S 中两个函数 __alltraps 和 __restore 的作用，并回答如下问题:

#### 1. L40：刚进入 __restore 时，sp 代表了什么值。请指出 __restore 的两种使用情景。
1. 刚进入_restore时，sp指向内核栈中TrapContex低地址的位置(此时内核栈的栈顶仅存在一个TrapContex)，也即指向TrapContex结构体，sp所指向的地址处存储的值是TrapContex中保存的x0寄存器值，又根据riscv规范中得知，x0寄存器永远为0，因此sp所指向的地址处存储的值是0。以sp为基地址，可以寻址到TrapContex里的所有内容。
2. _restore有两种使用情景：
   1. **在首次执行app时引导控制流**：首在os刚启动，将应用程序加载进入对应内存区域后(APP_BASE_ADDRESS + app_id * APP_SIZE_LIMIT)，会在每个应用的内核栈栈顶压入一个精心构造好的初始TrapContex，该TrapContex里的sp指向用户栈，sepc指向入口地址，sstatus为U特权级，其余通用寄存器为0。每个应用首次启动时，只需要使用_restore函数将这个TrapContex恢复即可进入应用的首行代码执行。
   2. **处理S特权级返回U特权级的切换流程**：当Trap控制流结束，需要返回app时，调用_restore返回应用程序的控制流。

#### 2. L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。
```
ld t0, 32*8(sp)
ld t1, 33*8(sp)
ld t2, 2*8(sp)
csrw sstatus, t0
csrw sepc, t1
csrw sscratch, t2
```
1. **sstatus**: SPP 位：指示trap返回时要进入S态还是U态（必须是U态）；SPIE 位：决定进入U态后中断是否使能。
2. **sepc**：记录 Trap 发生之前执行的最后一条指令的地址，告诉CPU trap返回 (sret) 时要跳转到用户态程序的PC。
3. **sscratch**：内核栈指针，用于 trap 返回内核时快速切换栈。

#### 3. L50-L56：为何跳过了 x2 和 x4？
```
ld x1, 1*8(sp)
ld x3, 3*8(sp)
.set n, 5
.rept 27
   LOAD_GP %n
   .set n, n+1
.endr
```
1. **跳过x2**：当执行完`csrrw sp, sscratch, sp`这条指令后，此时的sp指向的是内核栈，需要保存进入TrapContex里的用户栈指针在sscratch里。然而程序并不能直接使用类似于`sd sscratch 2*8(sp)`这样的一条指令将sscratch里的内容存入TrapContex中，而是需要将是sscratch读入临时寄存器，再将临时寄存器的值存入TrapContex。因此，需要等待临时寄存器存入TrapContex后，才能保存sscratch。
2. **跳过x4**：RISC-V ABI里，x4用作线程本地存储（TLS）指针。在本实验中暂时没用到 TLS 机制，内核也不保存/恢复它。

#### 4. L60：该指令之后，sp 和 sscratch 中的值分别有什么意义？
```
csrrw sp, sscratch, sp
```
- 该指令的实际作用是交换sp与sscratch的内容
- 执行该指令之后，sscratch指向内核栈，sp恢复为指向用户栈

#### 5. __restore：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？
- 执行完成`sret`指令之后，才完成状态切换
- sret指令会返回到sstatus所标志的模式，在此之前sstatus标记为用户态，因此sret会返回为用户态

#### 6. L13：该指令之后，sp 和 sscratch 中的值分别有什么意义？
```
csrrw sp, sscratch, sp
```
- 该指令的实际作用是交换sp与sscratch的内容
- 执行该指令之后，sscratch保存着用户栈，sp指向内核栈

#### 7. 从 U 态进入 S 态是哪一条指令发生的？
应用程序使用指令`ecal`，陷入内核态