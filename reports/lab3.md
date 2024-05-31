# rCore-2024S - Lab3 - report
@Author :    abcd1234  
@Time   :    2024/5/31, 18:03  
@Emaile :    abcd1234dbren@yeah.net  
## 我实现的功能
1. 照着fork和exec仿写，完成了spawn。
2. 完成了stride调度算法。
## 问答题
### 实际是p1执行吗？为什么？
不是，因为p2的priority是250，再增加10后就变为5了，仍然小于p1的255，所以实际是p2继续执行。
### 证明STRIDE_MAX – STRIDE_MIN <= BigStride / 2
不太会
### 实现partial_cmp
```rust
impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.0 > other.0 && self.0 - other.0 <= BigStride / 2 { 
            return Some(Ordering::Greater)
        } else if self.0 > other.0 && self.0 - other.0 > BigStride / 2 {
            return Some(Ordering::Less)
        } else if self.0 < other.0 && other.0 - self.0 <= BigStride / 2 {
            return Some(Ordering::Less)
        } else if self.0 < other.0 && other.0 - self.0 > BigStride / 2 {
            return Some(Ordering::Greater)
        }
    }
}
```
## 荣誉准则
1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：  
*无*

2. 此外，我也参考了以下资料，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：  
*无*

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。

## 看法
这部分在我看来难度较低，和ch3差不多了。

在这一次，主要我学的最多的是git的cherrypick，我个人感觉这东西真容易出事！极其不智能！因为ch4里我有一些内容本地能过，但交上去后远程没过，于是我在本地进行了调试，但之后再次commit和push上去的就不全了，这就很恶心。当我cherrypick时，用最新版本的复制不全，用老版本的还得重新改一遍代码，极其折磨。

后续，我通过手动复制粘贴的方法，解决了ch4的代码迁移问题。
