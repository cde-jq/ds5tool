# ds5tool

一个用 Rust 写的 Windows 小工具：检测 Sony DualSense（PS5 手柄），支持以下方式触发 `config.toml` 中配置的命令：

- **PS + L1 / L2 / R1 / R2 持续按住超过设定时间**（每个组合键可单独配置时长）
- **手柄连接成功时**（可选）

另外，**PS + Create（触摸板左侧按键）持续按住 2 秒**会停止最后一次启动的非 shell 命令；还可以打开开关，把最后一次由组合键启动的非 shell 命令记录到文件，下次启动后的首次手柄连接时自动恢复。

## 功能

- 通过 HID 直接读取 DualSense，不依赖 XInput
- 支持 USB 和蓝牙连接
- 支持 DualSense（PID `0x0CE6`）和 DualSense Edge（PID `0x0DF2`）
- 支持热插拔：手柄断开后自动等待重连
- 支持 `PS + L1`、`PS + L2`、`PS + R1`、`PS + R2` 四个组合键，全部可选
- 每个组合键独立计时、独立命令、独立按住时长
- 按住组合键期间只触发一次，松开后才允许再次触发
- 按住 `PS + Create` 2 秒可停止最后一次启动的**非 shell** 命令
- 可选：记住最后一次由组合键启动的非 shell 命令，下次启动读取，手柄首次连接时优先执行
- 手柄首次连接成功时可执行一条命令（程序运行期间只触发一次，断开重连不再执行）
- 命令、按住时长、工作目录、是否隐藏窗口均可配置

## 构建

需要：

1. Rust（MSVC 工具链，`stable-x86_64-pc-windows-msvc`）
2. Visual Studio Build Tools（包含 C++ 生成工具，`hidapi` 的 Windows 后端需要编译少量 C 代码）

```powershell
cargo build --release
```

生成的文件：

```
target\release\ds5tool.exe
```

部署时把 `ds5tool.exe` 和 `config.toml` 放在同一个目录即可。

## 运行

```powershell
.\target\release\ds5tool.exe
```

程序会先读取当前目录的 `config.toml`；如果当前目录没有，则读取 exe 所在目录的 `config.toml`。也可以显式指定：

```powershell
.\target\release\ds5tool.exe D:\path\to\config.toml
```

列出系统当前所有 HID 设备（排查识别问题）：

```powershell
.\target\release\ds5tool.exe --list
```

正常启动后输出类似：

```
配置文件: D:\code\ds5tool\config.toml
ds5tool 已启动
已加载 2 个组合键：
  PS + L1：按住 3.0 秒，命令: notepad.exe
  PS + R2：按住 1.0 秒，命令: shutdown /s /t 0
已配置手柄连接时执行命令（仅首次连接）: C:\Tools\on-connect.exe ["--device", "dualsense"]
停止功能：按住 PS + Create（触摸板左侧）2.0 秒，停止最后一次启动的非 shell 命令。
等待 DualSense 连接...
已连接: Wireless Controller (序列号 ...)
执行手柄连接命令...
[连接] 命令已启动 (PID 11111)
检测到 PS + L1，开始计时...
PS + L1 已按住 3.0 秒，执行命令。
[PS+L1] 命令已启动 (PID 12345)
PS + L1 已松开，计时重置
检测到 PS + Create，开始停止计时...
PS + Create 已按住 2.0 秒，停止最后一次启动的非 shell 命令。
已停止最后一次非 shell 命令 (PID 12345)。
PS + Create 已松开，停止计时重置
```

## 配置

`config.toml` 用 `[combos.<按键>]` 配置组合键。四个组合键都是可选的，不需要的整段注释掉即可。

```toml
# 组合键默认的按住时长（毫秒），可在每个 [combos.*] 中单独覆盖
hold_ms = 3000

# ---------------- PS + L1 ----------------
[combos.l1]
shell = true
command = 'notepad.exe'
args = []
# workdir = "C:\\path\\to\\dir"
# hide_window = true

# ---------------- PS + L2（可选） ----------------
# [combos.l2]
# hold_ms = 2000
# shell = true
# command = 'C:\Tools\example-l2.exe'

# ---------------- PS + R1（可选） ----------------
# [combos.r1]
# command = 'C:\Tools\example-r1.exe'

# ---------------- PS + R2（可选） ----------------
# [combos.r2]
# hold_ms = 1000
# command = 'C:\Tools\example-r2.exe'
```

顶层配置：

| 字段 | 是否必填 | 说明 |
| --- | --- | --- |
| `hold_ms` | 否 | 组合键默认按住时长，默认 3000 毫秒 |
| `remember_last_command` | 否 | 默认 `false`；设为 `true` 时记录最后一次组合键启动的非 shell 命令 |

每个组合键支持以下字段：

| 字段 | 是否必填 | 说明 |
| --- | --- | --- |
| `hold_ms` | 否 | 按住多久触发，默认使用顶层 `hold_ms`（3000 毫秒） |
| `shell` | 否 | 默认 `true`，通过 `cmd.exe /C` 执行 |
| `command` | 是 | 要执行的命令或程序 |
| `args` | 否 | 传给命令的参数，默认 `[]` |
| `workdir` | 否 | 工作目录 |
| `hide_window` | 否 | 默认 `true`，隐藏命令的控制台窗口 |

### 示例：四个组合键执行不同命令

```toml
hold_ms = 3000

[combos.l1]
command = 'notepad.exe'

[combos.l2]
hold_ms = 1500
shell = true
command = 'shutdown /s /t 0'

[combos.r1]
shell = false
command = 'C:\Tools\app.exe'
args = ['--mode', 'one']
workdir = 'C:\Tools'

[combos.r2]
hold_ms = 800
shell = true
command = 'powershell -NoProfile -WindowStyle Hidden -Command "Stop-Process -Name SomeApp"'
```

### 示例：启动一个程序

```toml
[combos.r1]
shell = false
command = 'C:\Program Files\SomeApp\SomeApp.exe'
args = ['--fullscreen']
workdir = 'C:\Program Files\SomeApp'
```

### 示例：执行 shell 命令

```toml
[combos.l2]
hold_ms = 2000
shell = true
command = 'shutdown /s /t 0'
```

### 示例：调用 PowerShell

```toml
[combos.r2]
shell = true
command = 'powershell -NoProfile -WindowStyle Hidden -Command "Stop-Process -Name SomeApp"'
hide_window = true
```

### 示例：手柄首次连接时执行命令

在配置文件末尾添加 `[on_connect]` 段：

```toml
[on_connect]
shell = false
command = 'C:\Tools\on-connect.exe'
args = ['--device', 'dualsense']
# workdir = 'C:\Tools'
hide_window = true
```

程序运行期间只在第一次成功打开手柄时执行一次，触发场景包括：

- 程序启动时手柄已经连接
- 启动后第一次插上 USB 或蓝牙连接成功

之后手柄断开再重连不会再次触发；想再次执行需要重启程序。

`[on_connect]` 内部支持 `shell`、`command`、`args`、`workdir`、`hide_window`，含义与 `[combos.*]` 相同。

### 兼容旧版配置

以前的顶层写法仍然有效，等价于 `[combos.l1]`：

```toml
hold_ms = 3000
shell = true
command = 'notepad.exe'
args = []
```

注意：不要同时使用顶层 `command` 和 `[combos.l1]`，程序会报错提示。

## 停止最后一次启动的命令（PS + Create）

按住 **PS + Create**（Create 是触摸板左侧的小按键）**2 秒**，程序会停止最后一次启动的**非 shell** 命令：

- 只跟踪 `shell = false` 的命令（包括组合键命令、`[on_connect]` 命令和恢复执行的命令）
- 只停止最近一次启动的那一个进程；启动新的非 shell 命令后，旧的句柄会被替换，旧进程不会被自动结束
- 如果该进程已经自行退出，会提示“已经结束，无需停止”
- 这个功能固定为 2 秒，不需要也不能通过配置修改
- 每次按住只停止一次，松开后才允许再次触发

> 注意：`shell = true` 的命令是通过 `cmd.exe /C` 启动的，实际程序往往是 `cmd.exe` 的子进程，无法通过 `cmd.exe` 的句柄可靠地停止，因此这类命令不会被跟踪。需要能被停止时请使用 `shell = false`。

> 注意：在蓝牙模式下，PS + Create 也是 DualSense 的配对快捷键，按住 2 秒可能同时进入配对状态。

## 记住最后一次命令（可选）

在 `config.toml` 顶层打开开关：

```toml
remember_last_command = true
```

启用后：

- 每次由组合键启动 **非 shell** 命令（`shell = false`）成功后，把 `command`、`args`、`workdir`、`hide_window` 写入配置文件同目录的 `last_command.toml`
- 程序启动时读取该文件，并在日志中显示恢复到的命令
- 程序启动后的首次手柄连接时，如果存在记录，优先执行记录的命令，并**跳过** `[on_connect]`；没有记录时才执行 `[on_connect]`
- 连接命令每次运行只执行一次，之后手柄断开重连不会再次触发
- `shell = true` 的命令、`[on_connect]` 启动的命令不会写入记录
- PS + Create 的停止功能同样适用于恢复执行的命令

记录文件是普通的 TOML，可以手动删除或修改；删除后下次启动就没有可恢复的命令：

```toml
command = "C:\\Tools\\app.exe"
args = ["--mode", "one"]
workdir = "C:\\Tools"
hide_window = true
```

启用并恢复成功时，启动日志类似：

```
持久化已启用：记录文件: D:\code\ds5tool\last_command.toml
已读取上次记录的命令: C:\Tools\app.exe ["--mode", "one"]
手柄首次连接时将优先执行该命令，并跳过 [on_connect]。
...
已连接: Wireless Controller (序列号 ...)
执行上次记录的命令: C:\Tools\app.exe ["--mode", "one"]
```

如果记录文件不存在，会显示“记录文件中暂无可恢复的命令。”，首次连接时仍按原来的 `[on_connect]` 逻辑执行。每次组合键成功启动命令后还会打印：

```
已记录命令: D:\code\ds5tool\last_command.toml
```

> 修改配置后需要重启程序。

## 实现原理

### 1. 找到手柄

DualSense 在 Windows 上以 HID 设备出现：

- Vendor ID：`0x054C`（Sony）
- Product ID：`0x0CE6`（DualSense）/ `0x0DF2`（DualSense Edge）
- 标准手柄 collection：Usage Page `0x01`、Usage `0x05`

代码用 `hidapi` 枚举设备，筛选出上述 VID/PID 和 usage，然后打开设备读取输入报告。

### 2. 解析按键

DualSense 的完整输入报告布局（公共部分）：

| 偏移（相对公共数据） | 内容 |
| --- | --- |
| 0 | 左摇杆 X |
| 1 | 左摇杆 Y |
| 2 | 右摇杆 X |
| 3 | 右摇杆 Y |
| 4 | L2 模拟量 |
| 5 | R2 模拟量 |
| 6 | 计数器 |
| 7 | `buttons[0]`：方向键 + 方块/叉/圆/三角 |
| 8 | `buttons[1]`：bit0 = L1，bit1 = R1，bit2 = L2，bit3 = R2，bit4 = Create |
| 9 | `buttons[2]`：bit0 = PS，bit1 = 触摸板，... |

不同连接方式的报告头不同：

- USB：report ID `0x01`，64 字节，公共数据从偏移 `1` 开始
- 蓝牙完整报告：report ID `0x31`，78 字节，公共数据从偏移 `2` 开始
- 蓝牙简化报告：report ID `0x01`，10 字节，`buttons[1]` 位于偏移 `6`，PS 位于偏移 `7`

因此代码根据 report ID 和长度计算按键位置：

```text
L1 = buttons[1] & 0x01
R1 = buttons[1] & 0x02
L2 = buttons[1] & 0x04
R2 = buttons[1] & 0x08
Create = buttons[1] & 0x10
PS = buttons[2] & 0x01
```

蓝牙连接时程序还会读取一次 feature report `0x09`，让手柄从简化报告切换到完整报告；即使这一步失败，简化报告也能正常识别这些组合键。

### 3. 计时与防重复

- 程序为每个已启用的组合键维护独立的 `pressed_at` 和 `fired` 状态
- 每次读到报告后检查 `PS && 对应按键`，第一次同时按下时记录 `Instant`
- 持续检查 `elapsed() >= hold_ms`
- 达到时间后执行该组合键对应的命令，并置 `fired = true`
- 只有松开该组合键后才把 `fired` 复位，因此按住不放不会重复执行
- 读取使用 200ms 超时；即使手柄暂时不发送新报告，也会在超时分支继续检查计时
- 多个组合键互不影响；如果同时按住多个，它们会分别计时并分别触发
- `PS + Create` 使用同样的计时逻辑，固定 2 秒后调用 `ProcessTracker::stop_last()`

### 4. 执行命令

默认通过 `cmd.exe /C` 执行，并使用 `CREATE_NO_WINDOW` 避免弹出黑色控制台窗口。需要看到控制台时把对应组合键的 `hide_window` 设为 `false`。

### 5. 连接时触发

程序在成功打开手柄后检查 `on_connect` 配置。程序运行期间只在**第一次**成功打开手柄时执行一次（包括启动时手柄已连接、启动后首次连接成功）；之后断开重连会跳过，不再执行。首次连接时如果启用了 `remember_last_command` 并且存在记录，会优先执行记录的命令并跳过 `[on_connect]`。连接命令和组合键命令共用同一套执行逻辑（`Action::execute`）。

### 6. 停止最后一次非 shell 命令

`ProcessTracker` 保存最近一次 `shell = false` 动作启动的 `Child`：

- `Action::execute` 只对非 shell 命令返回 `Child`，shell 命令返回 `None`
- 组合键命令通过 `ProcessTracker::run` 执行，`[on_connect]` 命令通过 `ProcessTracker::run_without_saving` 执行；两者都会参与 PS + Create 的停止跟踪
- 按住 `PS + Create` 2 秒时调用 `ProcessTracker::stop_last()`：先用 `try_wait()` 判断进程是否还活着，再用 `kill()` 结束它
- 启动新的非 shell 命令会替换旧句柄，不主动结束旧进程

### 7. 记住并恢复最后一次命令

- `remember_last_command = true` 时，`ProcessTracker` 会把组合键启动的非 shell 命令写入与 `config.toml` 同目录的 `last_command.toml`
- 文件内容由 `SavedCommand` 表示，只保存 `command`、`args`、`workdir`、`hide_window`；恢复时固定按 `shell = false` 执行
- 程序启动时 `ProcessTracker::new` 尝试读取该文件，成功则保存在 `saved` 字段并打印日志；文件不存在或格式错误只是忽略并给出提示
- 组合键命令通过 `ProcessTracker::run` 执行并更新记录；`[on_connect]` 通过 `ProcessTracker::run_without_saving` 执行，不写入文件
- 首次连接时先调用 `ProcessTracker::run_saved()`；返回 `true` 表示已执行恢复命令，此时跳过 `[on_connect]`；否则按原逻辑执行 `[on_connect]`
- 主循环用 `connect_action_done` 标记连接命令是否已执行，执行过一次后断开重连不会再次触发
- 恢复执行的命令也会作为“最后一次非 shell 命令”被跟踪，因此可以用 PS + Create 停止

## 开机自启 / 后台运行

最简单的方式：

1. `Win + R` 输入 `shell:startup`
2. 在打开的启动目录里放一个 `ds5tool.exe` 的快捷方式
3. 如果想隐藏程序自己的控制台窗口，可以在快捷方式属性里把“运行方式”设为“最小化”

也可以使用任务计划程序，在用户登录时启动：

- 触发器：登录时
- 操作：启动 `ds5tool.exe`
- 不要把任务设为“不管用户是否登录都运行”，否则命令会在 Session 0 中执行，无法启动用户桌面上的程序

如果希望程序完全无控制台窗口，可以在 `src/main.rs` 顶部加：

```rust
#![windows_subsystem = "windows"]
```

但这样 `println!/eprintln!` 日志也会消失，建议同时改成写日志文件。

## 注意事项

- **不要同时使用会独占手柄的软件**：如果 DS4Windows、Steam Input 或其它工具以独占方式打开了手柄，本程序可能无法读取。关掉独占模式后再试。
- **只监控第一个匹配的 DualSense**：同时连接多个 DualSense 时，程序只会打开第一个。
- **多个组合键同时按住**：每个组合键独立计时，同时按住会分别触发各自的命令。
- **L2/R2 使用的是数字按下位**：不是 0-255 的模拟量；按下扳机超过手柄固件的阈值后即视为按下。
- **PS + Create 也是蓝牙配对快捷键**：蓝牙模式下按住 2 秒，手柄可能同时进入配对状态。
- **只能停止非 shell 命令**：`shell = true` 的命令由 `cmd.exe /C` 启动，程序无法可靠地停止其真正的子进程。
- **只停止最近一次启动的非 shell 命令**：如果之后又启动了新的非 shell 命令，再按 PS + Create 只会停止新的那个。
- **只记录组合键命令**：`remember_last_command` 只保存组合键启动的非 shell 命令，`shell = true` 的命令和 `[on_connect]` 命令不会写入记录文件。
- **恢复命令会跳过 `[on_connect]`**：启用记录且存在记录时，首次连接只执行记录的命令；想恢复原始连接行为，删除 `last_command.toml` 即可。
- **连接命令只在首次连接执行**：恢复的命令和 `[on_connect]` 都只在程序运行期间第一次成功连接手柄时触发，断开重连不会再次执行；需要再次触发请重启程序。
- **记录文件可手动编辑**：`last_command.toml` 是普通 TOML，可以修改或删除；格式错误时程序会提示并忽略该记录。
- **记录文件会被自动执行**：启用后，`last_command.toml` 中的命令会在程序运行后的首次手柄连接时自动运行，请不要把不可信内容写入该文件。
- **命令权限**：程序以当前用户权限运行；需要管理员权限的命令请以管理员身份启动它。
- **PS 键**：程序读取的是 HID 原始报告中的 PS 键，不依赖 Windows 的游戏控制器映射。
- **蓝牙延迟/省电**：部分蓝牙适配器会在空闲时降低轮询率；如果计时不准确，优先使用 USB 连接。
- **Windows 服务**：不建议把它做成 Windows 服务，因为服务运行在 Session 0，无法在当前用户桌面启动程序。

## 代码结构

```
src/main.rs     HID 设备查找、按键解析、组合键计时、PS + Create 停止、恢复命令
src/config.rs   config.toml 读取、组合键动作、命令执行、进程跟踪与命令记录
config.toml     默认配置文件
last_command.toml  启用 remember_last_command 后自动生成的命令记录
```

## 可以继续扩展的方向

- 用 `tray-icon` 加系统托盘图标和退出菜单
- 支持任意按键组合（例如 PS + 叉、PS + 触摸板）映射到不同命令
- 配置文件热重载
- 写日志文件而不是控制台
- 同时监控多个手柄
- 触发时给手柄一个灯光/震动反馈
