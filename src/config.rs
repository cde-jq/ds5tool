use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// 解析后的配置。
#[derive(Debug)]
pub struct Config {
    /// 已启用的 PS + 按键组合。
    pub bindings: Vec<Binding>,
    /// 手柄连接时执行的动作。
    pub on_connect: Option<Action>,
}

/// 一个已启用的组合键绑定。
#[derive(Debug)]
pub struct Binding {
    pub button: Button,
    pub hold_ms: u64,
    pub action: Action,
}

/// 可以和 PS 组合的按键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    L1,
    L2,
    R1,
    R2,
}

impl Button {
    pub fn name(self) -> &'static str {
        match self {
            Button::L1 => "L1",
            Button::L2 => "L2",
            Button::R1 => "R1",
            Button::R2 => "R2",
        }
    }
}

/// 一个可配置的动作：执行什么命令、怎么执行。
#[derive(Debug, Deserialize)]
pub struct Action {
    /// true: 通过 `cmd.exe /C` 执行 command；false: 直接启动 command。
    #[serde(default = "default_shell")]
    pub shell: bool,

    /// 要执行的命令。
    pub command: String,

    /// 传给命令的参数。
    #[serde(default)]
    pub args: Vec<String>,

    /// 命令的工作目录（可选）。
    #[serde(default)]
    pub workdir: Option<PathBuf>,

    /// 是否用 CREATE_NO_WINDOW 启动命令，避免弹出控制台窗口。
    #[serde(default = "default_true")]
    pub hide_window: bool,
}

/// 和 TOML 文件结构一一对应的原始配置。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    /// 组合键默认的按住时长（毫秒）。
    #[serde(default = "default_hold_ms")]
    hold_ms: u64,

    // 旧版顶层 PS + L1 配置，继续兼容。
    #[serde(default = "default_shell")]
    shell: bool,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    workdir: Option<PathBuf>,
    #[serde(default = "default_true")]
    hide_window: bool,

    #[serde(default)]
    combos: RawCombos,

    #[serde(default)]
    on_connect: Option<Action>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCombos {
    #[serde(default)]
    l1: Option<RawBinding>,
    #[serde(default)]
    l2: Option<RawBinding>,
    #[serde(default)]
    r1: Option<RawBinding>,
    #[serde(default)]
    r2: Option<RawBinding>,
}

#[derive(Debug, Deserialize)]
struct RawBinding {
    /// 不写时使用顶层 hold_ms。
    #[serde(default)]
    hold_ms: Option<u64>,
    #[serde(flatten)]
    action: Action,
}

fn default_hold_ms() -> u64 {
    3000
}

fn default_shell() -> bool {
    true
}

fn default_true() -> bool {
    true
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        let text = fs::read_to_string(&path)
            .with_context(|| format!("无法读取配置文件 {}", path.display()))?;
        let config =
            Self::parse(&text).with_context(|| format!("配置文件格式错误 {}", path.display()))?;

        println!("配置文件: {}", path.display());
        Ok(config)
    }

    fn parse(text: &str) -> Result<Self> {
        let raw: RawConfig = toml::from_str(text)?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: RawConfig) -> Result<Self> {
        let mut bindings = Vec::new();

        // 旧写法：顶层 command 等价于 PS + L1。
        if let Some(command) = raw.command {
            if raw.combos.l1.is_some() {
                bail!("PS + L1 同时使用了顶层 command 和 [combos.l1]，请只保留一种写法");
            }

            let action = Action {
                shell: raw.shell,
                command,
                args: raw.args.clone(),
                workdir: raw.workdir.clone(),
                hide_window: raw.hide_window,
            };
            action.validate("command")?;
            bindings.push(Binding {
                button: Button::L1,
                hold_ms: raw.hold_ms,
                action,
            });
        }

        for (button, binding) in [
            (Button::L1, raw.combos.l1),
            (Button::L2, raw.combos.l2),
            (Button::R1, raw.combos.r1),
            (Button::R2, raw.combos.r2),
        ] {
            if let Some(binding) = binding {
                let field = format!("combos.{}.command", button.name().to_ascii_lowercase());
                binding.action.validate(&field)?;
                bindings.push(Binding {
                    button,
                    hold_ms: binding.hold_ms.unwrap_or(raw.hold_ms),
                    action: binding.action,
                });
            }
        }

        if let Some(on_connect) = &raw.on_connect {
            on_connect.validate("on_connect.command")?;
        }

        if bindings.is_empty() && raw.on_connect.is_none() {
            bail!("没有配置任何动作：请设置 [combos.*] 或 [on_connect]");
        }

        Ok(Self {
            bindings,
            on_connect: raw.on_connect,
        })
    }
}

impl Action {
    fn validate(&self, field: &str) -> Result<()> {
        if self.command.trim().is_empty() {
            bail!("配置项 {field} 不能为空");
        }
        Ok(())
    }

    /// 执行动作，`label` 用于日志中标识触发来源。
    ///
    /// 非 shell 命令会返回 `Child`，调用方可以保存它以便之后停止；
    /// shell 命令返回 `None`（实际的程序可能是 cmd.exe 的子进程，
    /// 无法通过 cmd.exe 的句柄可靠地停止）。
    pub fn execute(&self, label: &str) -> Option<Child> {
        let mut command = if self.shell {
            let mut cmd = Command::new("cmd.exe");
            cmd.arg("/C").arg(&self.command);
            cmd
        } else {
            Command::new(&self.command)
        };

        command.args(&self.args);

        if let Some(dir) = &self.workdir {
            command.current_dir(dir);
        }

        // 避免执行命令时闪出一个控制台窗口。需要交互式控制台时把
        // hide_window 改成 false。
        #[cfg(windows)]
        if self.hide_window {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        match command.spawn() {
            Ok(child) => {
                println!("[{label}] 命令已启动 (PID {})", child.id());
                if self.shell {
                    None
                } else {
                    Some(child)
                }
            }
            Err(err) => {
                eprintln!("[{label}] 启动命令失败: {err}");
                None
            }
        }
    }
}

/// 跟踪最后一次启动的非 shell 命令，供 PS + Create 停止。
#[derive(Default)]
pub struct ProcessTracker {
    last: Option<Child>,
}

impl ProcessTracker {
    /// 执行动作，并记住最后一次启动的非 shell 命令。
    pub fn run(&mut self, action: &Action, label: &str) {
        if let Some(child) = action.execute(label) {
            self.last = Some(child);
        }
    }

    /// 停止最后一次启动的非 shell 命令。
    ///
    /// 返回值表示是否真正结束了仍在运行的进程（没有可停止的命令、
    /// 命令已经退出或结束失败时返回 false）。
    pub fn stop_last(&mut self) -> bool {
        let Some(mut child) = self.last.take() else {
            println!("没有可停止的非 shell 命令。");
            return false;
        };

        match child.try_wait() {
            Ok(Some(_)) => {
                println!("最后一次非 shell 命令已经结束，无需停止。");
                false
            }
            Ok(None) => match child.kill() {
                Ok(()) => {
                    let _ = child.wait();
                    println!("已停止最后一次非 shell 命令 (PID {})。", child.id());
                    true
                }
                Err(err) => {
                    eprintln!("停止命令失败 (PID {}): {err}", child.id());
                    false
                }
            },
            Err(err) => {
                eprintln!("检查命令状态失败: {err}");
                false
            }
        }
    }
}

/// 配置文件查找顺序：
/// 1. 命令行第一个参数
/// 2. 当前目录下的 config.toml
/// 3. exe 所在目录下的 config.toml
fn config_path() -> PathBuf {
    if let Some(arg) = std::env::args_os().nth(1) {
        return PathBuf::from(arg);
    }

    let cwd = PathBuf::from("config.toml");
    if cwd.is_file() {
        return cwd;
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside_exe = dir.join("config.toml");
            if beside_exe.is_file() {
                return beside_exe;
            }
        }
    }

    cwd
}

#[cfg(test)]
mod tests {
    use super::{Action, Button, Config, ProcessTracker};

    #[test]
    fn parses_legacy_top_level_l1() {
        let config = Config::parse(
            r#"
hold_ms = 1500
shell = false
command = 'C:\Tools\trigger.exe'
args = ['--now']
hide_window = false
"#,
        )
        .unwrap();

        assert_eq!(config.bindings.len(), 1);
        let binding = &config.bindings[0];
        assert_eq!(binding.button, Button::L1);
        assert_eq!(binding.hold_ms, 1500);
        assert!(!binding.action.shell);
        assert_eq!(binding.action.command, r"C:\Tools\trigger.exe");
        assert_eq!(binding.action.args, vec!["--now"]);
        assert!(!binding.action.hide_window);
    }

    #[test]
    fn parses_all_combos_with_hold_overrides() {
        let config = Config::parse(
            r#"
hold_ms = 3000

[combos.l1]
command = 'one.exe'

[combos.l2]
hold_ms = 1200
command = 'two.exe'

[combos.r1]
command = 'three.exe'

[combos.r2]
hold_ms = 800
command = 'four.exe'
"#,
        )
        .unwrap();

        assert_eq!(config.bindings.len(), 4);
        assert_eq!(config.bindings[0].button, Button::L1);
        assert_eq!(config.bindings[0].hold_ms, 3000);
        assert_eq!(config.bindings[1].button, Button::L2);
        assert_eq!(config.bindings[1].hold_ms, 1200);
        assert_eq!(config.bindings[2].button, Button::R1);
        assert_eq!(config.bindings[2].hold_ms, 3000);
        assert_eq!(config.bindings[3].button, Button::R2);
        assert_eq!(config.bindings[3].hold_ms, 800);
    }

    #[test]
    fn combos_are_optional() {
        let config = Config::parse(
            r#"
[combos.r2]
command = 'only.exe'
"#,
        )
        .unwrap();

        assert_eq!(config.bindings.len(), 1);
        assert_eq!(config.bindings[0].button, Button::R2);
        assert_eq!(config.bindings[0].hold_ms, 3000);
    }

    #[test]
    fn rejects_duplicate_l1_definition() {
        let result = Config::parse(
            r#"
command = 'legacy.exe'

[combos.l1]
command = 'new.exe'
"#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn allows_on_connect_without_combos() {
        let config = Config::parse(
            r#"
[on_connect]
command = 'notepad.exe'
"#,
        )
        .unwrap();

        assert!(config.bindings.is_empty());
        assert!(config.on_connect.is_some());
    }

    #[test]
    fn rejects_empty_config() {
        assert!(Config::parse("").is_err());
    }

    #[test]
    fn parses_on_connect_action() {
        let config = Config::parse(
            r#"
[combos.l1]
command = 'notepad.exe'

[on_connect]
command = 'notepad.exe'
args = ['hello.txt']
"#,
        )
        .unwrap();

        let on_connect = config.on_connect.unwrap();
        assert_eq!(on_connect.command, "notepad.exe");
        assert_eq!(on_connect.args, vec!["hello.txt"]);
        assert!(on_connect.shell);
        assert!(on_connect.hide_window);
    }

    fn action(shell: bool, command: &str, args: &[&str]) -> Action {
        Action {
            shell,
            command: command.to_string(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            workdir: None,
            hide_window: true,
        }
    }

    #[test]
    fn shell_action_does_not_return_child() {
        assert!(action(true, "exit 0", &[]).execute("测试").is_none());
    }

    #[test]
    fn non_shell_action_returns_child() {
        let mut child = action(false, "cmd.exe", &["/C", "exit"])
            .execute("测试")
            .expect("非 shell 命令应返回 Child");
        let _ = child.wait();
    }

    #[test]
    fn tracker_stops_last_non_shell_command() {
        let mut tracker = ProcessTracker::default();
        tracker.run(
            &action(false, "cmd.exe", &["/C", "ping -n 20 127.0.0.1 >nul"]),
            "测试",
        );
        assert!(tracker.stop_last());
        assert!(!tracker.stop_last(), "句柄已被移除，不应再次停止");
    }
}
