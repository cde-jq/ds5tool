mod config;

use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use config::{Action, Button, Config, ProcessTracker};
use hidapi::{BusType, HidApi, HidDevice};

/// Sony 的 USB 厂商 ID。
const SONY_VENDOR_ID: u16 = 0x054c;
/// DualSense (0x0CE6) 和 DualSense Edge (0x0DF2)。
const DUALSENSE_PRODUCT_IDS: [u16; 2] = [0x0ce6, 0x0df2];

/// HID Usage Page: Generic Desktop
const HID_USAGE_PAGE_GENERIC_DESKTOP: u16 = 0x01;
/// HID Usage: Game Pad
const HID_USAGE_GAMEPAD: u16 = 0x05;

/// 按住 PS + Create 多久后停止最后一次启动的非 shell 命令（毫秒）。
const STOP_HOLD_MS: u64 = 2000;

fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--list") {
        return list_hid_devices();
    }

    let config = Config::load()?;

    println!("ds5tool 已启动");
    if config.bindings.is_empty() {
        println!("未配置组合键命令。");
    } else {
        println!("已加载 {} 个组合键：", config.bindings.len());
        for binding in &config.bindings {
            println!(
                "  PS + {}：按住 {:.1} 秒，命令: {}",
                binding.button.name(),
                binding.hold_ms as f64 / 1000.0,
                command_summary(&binding.action)
            );
        }
    }
    if let Some(action) = &config.on_connect {
        println!("已配置手柄连接时执行命令: {}", command_summary(action));
    }
    println!(
        "停止功能：按住 PS + Create（触摸板左侧）{:.1} 秒，停止最后一次启动的非 shell 命令。",
        STOP_HOLD_MS as f64 / 1000.0
    );

    let mut tracker = ProcessTracker::default();
    let mut waiting = false;

    loop {
        let api = match HidApi::new() {
            Ok(api) => api,
            Err(err) => {
                eprintln!("初始化 HID 失败: {err}");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        let Some(info) = find_controller(&api) else {
            if !waiting {
                println!("等待 DualSense 连接...");
                waiting = true;
            }
            thread::sleep(Duration::from_secs(1));
            continue;
        };

        waiting = false;

        let path = info.path().to_owned();
        let name = info.product_string().unwrap_or("DualSense").to_string();
        let serial = info.serial_number().unwrap_or("").to_string();
        let is_bluetooth = matches!(info.bus_type(), BusType::Bluetooth);

        let device = match api.open_path(&path) {
            Ok(device) => device,
            Err(err) => {
                eprintln!("无法打开 {name}: {err}");
                thread::sleep(Duration::from_secs(1));
                continue;
            }
        };

        if serial.is_empty() {
            println!("已连接: {name}");
        } else {
            println!("已连接: {name} (序列号 {serial})");
        }

        if let Some(action) = &config.on_connect {
            println!("执行手柄连接命令...");
            tracker.run(action, "连接");
        }

        // 蓝牙连接默认可能只发送 10 字节的简化报告。读取一次 feature
        // report 0x09 会让手柄切换到带完整按键/摇杆数据的 0x31 报告。
        // 即使这一步失败，下面的解析逻辑也能处理简化报告。
        if is_bluetooth {
            enable_bluetooth_full_reports(&device);
        }

        if let Err(err) = watch(device, &config, &mut tracker) {
            eprintln!("手柄已断开: {err:#}");
        }
        println!("等待 DualSense 重新连接...");
        thread::sleep(Duration::from_secs(1));
    }
}

fn command_summary(action: &Action) -> String {
    if action.args.is_empty() {
        action.command.clone()
    } else {
        format!("{} {:?}", action.command, action.args)
    }
}

fn find_controller(api: &HidApi) -> Option<&hidapi::DeviceInfo> {
    let is_dualsense = |info: &&hidapi::DeviceInfo| {
        info.vendor_id() == SONY_VENDOR_ID && DUALSENSE_PRODUCT_IDS.contains(&info.product_id())
    };

    // 优先选择标准手柄 collection；某些系统上 usage 信息可能拿不到，
    // 此时退回到同 VID/PID 的第一个设备。
    api.device_list()
        .find(|info| {
            is_dualsense(info)
                && info.usage_page() == HID_USAGE_PAGE_GENERIC_DESKTOP
                && info.usage() == HID_USAGE_GAMEPAD
        })
        .or_else(|| api.device_list().find(is_dualsense))
}

/// 每个组合键独立的计时状态。
#[derive(Default)]
struct HoldState {
    /// 组合键第一次被按下的时间；None 表示当前没有同时按住。
    pressed_at: Option<Instant>,
    /// 本次按住是否已经触发过，避免按住不放时反复执行。
    fired: bool,
}

/// `update_press` 检测到的状态变化。
enum PressEvent {
    /// 本次按住开始。
    Started,
    /// 本次按住结束。
    Released,
}

/// 阻塞读取手柄报告，并在任意 PS + L1/L2/R1/R2 持续按住时触发命令，
/// 在 PS + Create 持续按住时停止最后一次启动的非 shell 命令。
fn watch(device: HidDevice, config: &Config, tracker: &mut ProcessTracker) -> Result<()> {
    let mut buffer = [0u8; 128];
    let mut states: Vec<HoldState> = config
        .bindings
        .iter()
        .map(|_| HoldState::default())
        .collect();
    let mut stop_state = HoldState::default();
    let stop_hold = Duration::from_millis(STOP_HOLD_MS);

    loop {
        let read = device
            .read_timeout(&mut buffer, 200)
            .context("读取 HID 输入报告失败")?;

        // read == 0 表示超时，没有新报告。超时也要检查计时，
        // 因为手柄状态不变时可能不会持续发送新报告。
        if read > 0 {
            if let Some(buttons) = parse_buttons(&buffer[..read]) {
                for (binding, state) in config.bindings.iter().zip(states.iter_mut()) {
                    let pressed = buttons.ps && buttons.has(binding.button);
                    match update_press(state, pressed) {
                        Some(PressEvent::Started) => {
                            println!("检测到 PS + {}，开始计时...", binding.button.name());
                        }
                        Some(PressEvent::Released) => {
                            println!("PS + {} 已松开，计时重置", binding.button.name());
                        }
                        None => {}
                    }
                }

                match update_press(&mut stop_state, buttons.ps && buttons.create) {
                    Some(PressEvent::Started) => println!("检测到 PS + Create，开始停止计时..."),
                    Some(PressEvent::Released) => println!("PS + Create 已松开，停止计时重置"),
                    None => {}
                }
            }
        }

        for (binding, state) in config.bindings.iter().zip(states.iter_mut()) {
            if state.fired {
                continue;
            }
            if let Some(since) = state.pressed_at {
                if since.elapsed() >= Duration::from_millis(binding.hold_ms) {
                    state.fired = true;
                    println!(
                        "PS + {} 已按住 {:.1} 秒，执行命令。",
                        binding.button.name(),
                        binding.hold_ms as f64 / 1000.0
                    );
                    tracker.run(&binding.action, &format!("PS+{}", binding.button.name()));
                }
            }
        }

        if !stop_state.fired {
            if let Some(since) = stop_state.pressed_at {
                if since.elapsed() >= stop_hold {
                    stop_state.fired = true;
                    println!(
                        "PS + Create 已按住 {:.1} 秒，停止最后一次启动的非 shell 命令。",
                        stop_hold.as_secs_f64()
                    );
                    tracker.stop_last();
                }
            }
        }
    }
}

fn update_press(state: &mut HoldState, pressed: bool) -> Option<PressEvent> {
    if pressed {
        if state.pressed_at.is_none() {
            state.pressed_at = Some(Instant::now());
            return Some(PressEvent::Started);
        }
    } else if state.pressed_at.take().is_some() {
        state.fired = false;
        return Some(PressEvent::Released);
    }
    None
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Buttons {
    ps: bool,
    l1: bool,
    l2: bool,
    r1: bool,
    r2: bool,
    /// Create（触摸板左侧的小按键）。
    create: bool,
}

impl Buttons {
    fn has(&self, button: Button) -> bool {
        match button {
            Button::L1 => self.l1,
            Button::L2 => self.l2,
            Button::R1 => self.r1,
            Button::R2 => self.r2,
        }
    }
}

/// 从 DualSense 的输入报告中解析 PS、L1、L2、R1、R2、Create 的状态。
///
/// USB 完整报告: report ID = 0x01，长度 64，公共数据从偏移 1 开始。
/// 蓝牙完整报告: report ID = 0x31，长度 78，公共数据从偏移 2 开始。
/// 公共数据布局: X, Y, Rx, Ry, L2, R2, 计数器, buttons[4]
/// buttons[1]: bit0 = L1，bit1 = R1，bit2 = L2，bit3 = R2，bit4 = Create
/// buttons[2]: bit0 = PS
///
/// 蓝牙的简化报告: report ID = 0x01，长度 10，buttons 位于偏移 6/7。
fn parse_buttons(report: &[u8]) -> Option<Buttons> {
    let id = report.first().copied()?;

    let (button_index, ps_index) = if id == 0x01 && report.len() == 10 {
        // 蓝牙简化报告
        (6, 7)
    } else {
        let payload_start = match id {
            0x01 => 1, // USB 完整报告
            0x31 => 2, // 蓝牙完整报告
            _ => return None,
        };

        let buttons = payload_start + 7; // 跳过 X,Y,Rx,Ry,L2,R2,计数器
        if report.len() < buttons + 3 {
            return None;
        }
        (buttons + 1, buttons + 2)
    };

    let bits = report[button_index];
    Some(Buttons {
        ps: report[ps_index] & 0x01 != 0,
        l1: bits & 0x01 != 0,
        r1: bits & 0x02 != 0,
        l2: bits & 0x04 != 0,
        r2: bits & 0x08 != 0,
        create: bits & 0x10 != 0,
    })
}

/// 请求 feature report 0x09，使蓝牙手柄从简化报告切换到完整报告。
/// 失败不影响基本功能，因为 parse_buttons 也能解析简化报告。
fn enable_bluetooth_full_reports(device: &HidDevice) {
    let mut buffer = [0u8; 64];
    buffer[0] = 0x09; // 配对信息 / 蓝牙地址

    if let Err(err) = device.get_feature_report(&mut buffer) {
        eprintln!("读取蓝牙 feature report 失败（可忽略）: {err}");
    }
}

fn list_hid_devices() -> Result<()> {
    let api = HidApi::new().context("初始化 HID 失败")?;

    for info in api.device_list() {
        println!(
            "VID {:04x} PID {:04x} usage {:04x}:{:04x}  {:<30}  {}",
            info.vendor_id(),
            info.product_id(),
            info.usage_page(),
            info.usage(),
            info.product_string().unwrap_or("-"),
            info.path().to_string_lossy()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_buttons, Buttons};

    fn usb_report(bits: u8, ps: bool) -> [u8; 64] {
        let mut report = [0u8; 64];
        report[0] = 0x01;
        report[9] = bits;
        report[10] = u8::from(ps);
        report
    }

    #[test]
    fn parses_all_usb_buttons() {
        assert_eq!(
            parse_buttons(&usb_report(0x0f, true)),
            Some(Buttons {
                ps: true,
                l1: true,
                l2: true,
                r1: true,
                r2: true,
                create: false,
            })
        );
        assert_eq!(
            parse_buttons(&usb_report(0x02, false)),
            Some(Buttons {
                ps: false,
                l1: false,
                l2: false,
                r1: true,
                r2: false,
                create: false,
            })
        );
    }

    #[test]
    fn parses_bluetooth_full_report() {
        let mut report = [0u8; 78];
        report[0] = 0x31;
        report[10] = 0x04; // L2
        report[11] = 0x01; // PS
        assert_eq!(
            parse_buttons(&report),
            Some(Buttons {
                ps: true,
                l1: false,
                l2: true,
                r1: false,
                r2: false,
                create: false,
            })
        );
    }

    #[test]
    fn parses_bluetooth_minimal_report() {
        let mut report = [0u8; 10];
        report[0] = 0x01;
        report[6] = 0x08; // R2
        report[7] = 0x01; // PS
        assert_eq!(
            parse_buttons(&report),
            Some(Buttons {
                ps: true,
                l1: false,
                l2: false,
                r1: false,
                r2: true,
                create: false,
            })
        );
    }

    #[test]
    fn parses_create_button() {
        assert_eq!(
            parse_buttons(&usb_report(0x10, true)),
            Some(Buttons {
                ps: true,
                l1: false,
                l2: false,
                r1: false,
                r2: false,
                create: true,
            })
        );

        let mut report = [0u8; 10];
        report[0] = 0x01;
        report[6] = 0x10; // Create
        report[7] = 0x01; // PS
        assert_eq!(
            parse_buttons(&report),
            Some(Buttons {
                ps: true,
                l1: false,
                l2: false,
                r1: false,
                r2: false,
                create: true,
            })
        );
    }

    #[test]
    fn ignores_other_reports() {
        assert_eq!(parse_buttons(&[0x02, 1, 2, 3]), None);
        assert_eq!(parse_buttons(&[]), None);
    }
}
