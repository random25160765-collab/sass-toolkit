# SASS 工具链项目章程（定稿）

> 定稿时间：2026-08-10 | 状态：v1.0 定稿
> 范围：cubit → sass-* 工具族 → libsass 底座 → 可插拔执行/验证后端

---

## 一、定位宣言

**造反路线**：完整逆向 NVIDIA SASS（SM120/Blackwell 起步），构建一套**不依赖 NVIDIA 硬件、不依赖 NVIDIA 软件栈**（nvcc、ptxas、CUDA toolkit、Nsight）的 SASS 工具链。

核心主张：**没有任何一个环节是不可替换的**。

| 层 | 解耦内容 | 不可替换性 |
|---|---|---|
| 硬件层 | 验证/执行后端可插拔，GPU 只是最高档 | GPU 可没有 |
| 入口层 | Unix 工具族管道组合，不搞 monolith | CLI 可拆分 |
| 生成器层 | LLM 只是候选生成器之一 | 生成器可换 |
| 编译器层 | 任意编译器产出的 cubin 都是输入 | 输入来源任意 |

唯一不可拆的两样：**内核（libsass）** 与 **验证闭环**。

---

## 二、六能全景

```
能查     blackwell-isa + 值矩阵 + isa_db            （数据库）
能读     cubit decoder                              （引擎）
能懂     lifter 148 规则 + ptxNinja                 （静态语义）
能改     cubit encoder + lower                      （引擎）
能优化   reforge/recolor + 规则 pass + 生成器框架    （闭环）
能调试   执行引擎 × 观测 × 定位 × 分析 × 验证        （工作流）
```

### 能调试的完整定义

调试不是「执行」本身，而是完整工作流：

```
能调试 = 执行引擎 × 调试工作流
├─ 执行    ocelot PTX 模拟 · zluda 异构硬件 · 真机差分     （引擎）
├─ 观测    插桩 dump（GPR/谓词/共享内存）                  （看状态）
├─ 定位    二分搜索分歧源头 · diff 对比                    （找问题）
├─ 分析    热力图 · 寄存器活动 · 双缓冲识别 · stall 预估    （理解行为）
└─ 验证    修复后重跑，diff 清零                           （闭环）
```

与「能懂」的分工：能懂回答「程序长什么样」（静态语义），能调试回答「程序跑起来发生了什么」（行为观测）。能调试同时是能优化的**质检线**——diff 清零是优化的入场券。

---

## 三、产品形态：Unix 工具族 + libsass 底座

**一组 CLI，不是一个大 CLI**。统一前缀保证可发现性，各自薄封装，共享同一内核库。

```
sass-* 命令族
├─ sass-disasm   cubin → SASS 文本        （cubit 内核）
├─ sass-asm      SASS 文本 → cubin        （cubit 内核）
├─ sass-lift     SASS → 语义 IR           （lifter 148 规则）
├─ sass-db       查 ISA 表                （blackwell-isa 数据）
├─ sass-opt      规则优化 pass            （cubit ptx_opt）
├─ sass-reforge  变异搜索优化             （sasskit reforge）
├─ sass-verify   差分验证，后端可插拔      （新）
├─ sass-recolor  寄存器重着色             （sasskit recolor）
├─ sass-run      执行/验证统一入口        （后端可插拔，见 §四）
├─ sass-stats    inst mix · occupancy · 寄存器数/block   （静态，零硬件）
├─ sass-profile  关键路径 · stall 预估 · 热点区间         （静态，零硬件）
├─ sass-instrument 插桩生成（收编 dump_instrument）       （配合执行后端）
├─ sass-bisect   分歧定位（收编 auto_bisect）             （配合执行后端）
└─ sass-heatmap  热力图/模式识别（收编 heatmap）          （静态+动态双模式）

共享底座：libsass（Rust crate）
  - 所有 sass-* 链接同一个库
  - 统一的是数据格式和 IR，不是入口
```

管道化天然成立：

```bash
# 理解一个陌生 kernel：无 GPU 三步走
sass-disasm mystery.cubin | sass-stats --mix --occupancy   # 它是什么结构
sass-disasm mystery.cubin | sass-heatmap --mode regs       # 寄存器双缓冲/流水线模式
sass-disasm mystery.cubin | sass-profile --critical-path   # 瓶颈在哪

# 一条流水线：反汇编 → 语义提升 → 规则优化 → 重新编码 → 验证
sass-disasm kernel.cubin -k my_kernel --frozen \
  | sass-lift \
  | sass-opt --pass fold_addr,peephole \
  | sass-asm --template kernel.cubin -o patched.cubin \
  | sass-verify --backend gpu patched.cubin kernel.cubin
```

分发按 apt metapackage 设计：`sass-toolkit` 依赖整组，或 `sass-toolkit-disasm` 单件按需安装。每个命令独立演进、独立发版；libsass 单独发版。

---

## 四、执行后端矩阵（可插拔 × 精度可调）

`sass-run` 是执行/验证的统一入口，四个后端全部由**已有资产或轻量静态计算**拼装，新增量只是胶水：

| 后端 | 来源 | 用途 | 新增工作量 |
|---|---|---|---|
| `ptx-interp` | gpuocelot（已有）+ lifter（已有） | 功能级验证，无 GPU 兜底，完全可复现 | 胶水 |
| `est` | cubit 调度模型（已有） | 周期预估（静态 latency 求和，非执行），优化候选筛选 | 胶水 |
| `zluda` | cuda-oxide（已有） | CUDA→AMD 硬件，异构加速 | 胶水 |
| `gpu` | 差分执行（已有） | NVIDIA 真机差分，最终裁决 | 胶水 |

关键决策：**功能级验证不需要自研 SASS 解释器**——cubin → lifter → PTX → ocelot 执行即可。PTX 语义层足够验证正确性，QMMA、atom、bar.sync 等特殊指令的语义由 lifter 负责，不重复建设。lifter 提升的正确性被这条链顺带验证（提升→执行→对比）。

**不建 SASS 功能级解释器**；SASS 指令级精度的唯一独特价值（周期预估）由静态 `est` 后端覆盖，不需要完整模拟器。

使用示例：

```bash
# 无 GPU 的优化验证：周期级预估
sass-disasm kernel.cubin | sass-opt | sass-asm | \
  sass-run --backend interp --prec cycle --compare baseline.cubin

# 有 GPU 时升级到最终裁决
sass-run patched.cubin --backend gpu --compare kernel.cubin

# 调试：单步 + 轨迹
sass-run kernel.cubin -k my_kernel --backend interp --step --trace > trace.txt
```

---

## 五、验证证据链（递进，不是替代）

| 档位 | 证明什么 | 环境 | 可复现性 |
|---|---|---|---|
| `ptx-interp` | 变换在 **PTX 语义下保持等价** | 无 GPU 兜底 | 完全可复现 |
| `est` | 变换的静态周期收益估算 | 无 GPU | 完全可复现 |
| `zluda` | 在该 AMD 硬件执行语义下等价 | 需 AMD 硬件 | 部分 |
| `gpu` | 在**真实硬件上逐位一致** | 需 NVIDIA GPU | 不可完全复现 |

三者是**递进证据链**，不是替代。论文措辞：无 GPU 环境用前两级做回归，有 GPU 用最高级做确认。

---

## 六、增值服务：Nsight 功能软件化

### 收编的三个脚本（已存在，原型可用）

| 脚本 | 能力 | 收编为 |
|---|---|---|
| `dump_instrument.py` | 任意位置插桩：bar.sync 后 dump GPR/谓词/uniform/共享内存 | `sass-instrument` |
| `auto_bisect.py` | 二分自动定位第一个分歧发生在哪个 barrier 对 | `sass-bisect` |
| `heatmap.py` | 热力图：指令密度、寄存器活动、gang 检测、double-buffer 识别 | `sass-heatmap` |

它们不是散件，合起来是完整的**动态分析链**，且从「验证 lifter」升级为「理解任意 kernel」的通用工具。

### Nsight 能力软件化边界（诚实声明）

| nsight 功能 | 软件实现 | 需要执行吗 |
|---|---|---|
| 指令混合（inst mix） | SASS 指令类型统计 | 否 |
| 理论 occupancy | 寄存器/共享内存/block 限制 → occupancy 表 | 否 |
| 寄存器压力热力图 | heatmap regs 模式（已有） | 否 |
| 热点区间/循环定位 | CFG + 指令密度（heatmap sass 模式） | 否 |
| 关键路径/依赖链 | 依赖图 + latency 表（cubit 调度模型） | 否 |
| stall 原因预估 | RAW/WAR/port 冲突静态判定 | 否 |
| bank conflict 检测 | 共享内存地址模式启发式 | 否（精确需执行） |
| barrier 同步状态 | 插桩 dump（已有） | 是 |
| 分歧定位 | auto_bisect（已有） | 是 |
| 时间线 profiling | 插桩打时间戳 | 是 |
| 真实 stall 周期/吞吐 | — | **做不到（需硬件 counter）** |

边界：凡依赖硬件 counter 的拿不到；但 nsight 价值一半以上是「读 SASS 讲出 kernel 结构」——静态面全部可算。**软件版 Nsight：静态面全做，动态面用插桩近似。**

---

## 七、诚实难度声明

- **功能级验证**：工作量可控（胶水为主），正确路径的起点。
- **周期级预估**：难在精确建模（端口争用、双发射、内存子系统）。先做粗粒度——基于 isa 表的指令级 latency 求和（cubit 调度器已有该模型），精度够筛候选即可，不追求 cycle-accurate。
- **特殊指令**：lifter 中 QMMA/tensor core、atom/red 原子性、bar.sync 同步、控制流是工作量大头。
- **建议路径**：功能级验证落地（无 GPU 化正确性闭环）→ 粗粒度周期估计跟上（优化筛选）→ 特殊指令逐步补全 → 真机验证按需接入。

---

## 八、与现有资产的关系

| 资产 | 位置 | 在本架构中的角色 |
|---|---|---|
| cubit | 本仓库 | 能读/能改的引擎，disasm/asm/encode/roundtrip/patch 内核 |
| blackwell-isa | vendored table | 能查的数据底座 |
| lifter 148 规则 | platform/sass-spec | 能懂的语义核心，sass-lift |
| sasskit（reforge/recolor） | 外部 | 能优化的变异搜索/寄存器重着色 |
| gpuocelot | 外部 | ptx-interp 执行后端 |
| cuda-oxide / zluda | 外部 | zluda 执行后端 |
| hetGPU 三脚本 | 本仓库 lib/qemu | 能调试的观测/定位/分析原型 |
| cubit ptx_opt / 调度模型 | 本仓库 | 能优化的规则 pass 与 est 周期模型 |

**不变项**：libsass 内核、验证闭环、六能全景。
