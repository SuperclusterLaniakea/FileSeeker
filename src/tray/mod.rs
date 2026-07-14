/// System tray module — Windows notification area icon
///
/// 使用 WinAPI 直接操作（winapi crate），不依赖任何第三方托盘库。
/// 在后台线程上创建一个隐藏窗口 + 消息循环，通过 mpsc channel
/// 将托盘事件传递回主线程。

use std::ffi::OsStr;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::process::{Command, Child};
use std::ptr::null_mut;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use winapi::shared::minwindef::{DWORD, HINSTANCE, LPARAM, LRESULT, UINT, WPARAM, FALSE};
use winapi::shared::windef::{HICON, HWND, POINT};
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::shellapi::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use winapi::um::winuser::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW,
    LoadImageW, PostMessageW, PostQuitMessage, RegisterClassExW, SetForegroundWindow,
    ShowWindow, TrackPopupMenu, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDI_APPLICATION, IMAGE_ICON,
    LR_LOADFROMFILE, MF_STRING, MSG, SW_HIDE, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_NULL,
    WM_RBUTTONUP, WM_USER, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};

// ── Constants ─────────────────────────────────────────────────────────────

/// 自定义消息 ID，用于托盘图标回调（WM_USER + 1 避免与系统消息冲突）。
const WM_TRAYICON: UINT = WM_USER + 1;

/// 托盘图标 ID。
const TRAY_ICON_ID: UINT = 1001;

/// 右键菜单命令 ID。
const CMD_SHOW: UINT = 1002;
const CMD_HIDE: UINT = 1003;
const CMD_EXIT: UINT = 1004;

// ── Public API ────────────────────────────────────────────────────────────

/// 托盘事件，由主线程轮询处理。
#[derive(Debug, Clone)]
pub enum TrayAction {
    ShowWindow,
    HideWindow,
    ExitApp,
    ToggleVisibility,
}

/// 全局 Sender，由后台线程的窗口过程使用。
static TRAY_TX: OnceLock<Sender<TrayAction>> = OnceLock::new();

/// 初始化系统托盘，在后台线程上创建隐藏窗口 + 消息循环。
/// 返回 `Receiver<TrayAction>`，主线程通过 `poll_tray_event` 轮询。
pub fn init_tray() -> Receiver<TrayAction> {
    let (tx, rx) = mpsc::channel();

    // 在 spawn 线程之前存入全局，确保窗口过程能立即使用
    let _ = TRAY_TX.set(tx.clone());

    std::thread::spawn(move || {
        tray_thread(tx);
    });

    rx
}

/// 非阻塞地检查是否有待处理的托盘事件。
pub fn poll_tray_event(rx: &Receiver<TrayAction>) -> Option<TrayAction> {
    rx.try_recv().ok()
}

// ── tray-helper.exe 生命周期管理 ──────────────────────────────────────────

/// 全局 tray-helper 子进程句柄。
static TRAY_HELPER: Mutex<Option<Child>> = Mutex::new(None);

/// 确保 tray-helper.exe 正在运行（首次最小化时调用）。
pub fn ensure_tray_helper() {
    let mut guard = TRAY_HELPER.lock().unwrap();
    if guard.is_some() {
        return;
    }
    let path = get_tray_helper_path();
    if path.exists() {
        match Command::new(&path).spawn() {
            Ok(child) => {
                *guard = Some(child);
            }
            Err(e) => {
                eprintln!("tray::ensure_tray_helper: failed to launch: {}", e);
            }
        }
    } else {
        eprintln!("tray::ensure_tray_helper: {} not found", path.display());
    }
}

/// 终止 tray-helper.exe（主进程退出时调用）。
pub fn kill_tray_helper() {
    if let Some(mut child) = TRAY_HELPER.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn get_tray_helper_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tray-helper.exe")
}

// ── 字符串工具 ────────────────────────────────────────────────────────────

/// 将 Rust 字符串转为以 `\0` 结尾的宽字符 Vec。
fn to_wstring(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// 将宽字符切片拷贝到定长缓冲区，最多 `max_len - 1` 个字符 + 末尾 `\0`。
unsafe fn copy_wstr_into(src: &[u16], dst: *mut u16, max_len: usize) {
    let mut i = 0usize;
    while i < max_len - 1 && i < src.len() && *src.as_ptr().add(i) != 0 {
        *dst.add(i) = *src.as_ptr().add(i);
        i += 1;
    }
    *dst.add(i) = 0;
}

// ── 托盘图标操作 ──────────────────────────────────────────────────────────

/// 尝试从 icon.ico 文件加载图标，失败时使用系统默认图标。
unsafe fn load_tray_icon() -> HICON {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    for name in &["icon.ico", "../icon.ico"] {
        let path = exe_dir.join(name);
        if path.exists() {
            let wpath = to_wstring(&path.to_string_lossy());
            let handle = LoadImageW(
                null_mut(),
                wpath.as_ptr(),
                IMAGE_ICON,
                32,
                32,
                LR_LOADFROMFILE,
            );
            if !handle.is_null() {
                return handle as HICON;
            }
        }
    }

    LoadIconW(null_mut(), IDI_APPLICATION)
}

/// 向系统通知区添加托盘图标。
unsafe fn add_tray_icon(hwnd: HWND) -> bool {
    let mut nid: NOTIFYICONDATAW = mem::zeroed();
    nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as DWORD;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ICON_ID;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = WM_TRAYICON;

    let tip = to_wstring("文件检索助手 - 文件搜索");
    copy_wstr_into(&tip, nid.szTip.as_mut_ptr(), 128);

    nid.hIcon = load_tray_icon();

    if Shell_NotifyIconW(NIM_ADD, &mut nid) == FALSE {
        return false;
    }
    true
}

/// 从系统通知区移除托盘图标。
unsafe fn remove_tray_icon(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = mem::zeroed();
    nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as DWORD;
    nid.hWnd = hwnd;
    nid.uID = TRAY_ICON_ID;
    Shell_NotifyIconW(NIM_DELETE, &mut nid);
}

// ── 右键菜单 ──────────────────────────────────────────────────────────────

/// 在鼠标当前位置弹出右键菜单。
unsafe fn show_context_menu(hwnd: HWND) {
    let hmenu = CreatePopupMenu();
    if hmenu.is_null() {
        return;
    }

    AppendMenuW(
        hmenu,
        MF_STRING,
        CMD_SHOW as usize,
        to_wstring("显示 文件检索助手").as_ptr(),
    );
    AppendMenuW(
        hmenu,
        MF_STRING,
        CMD_HIDE as usize,
        to_wstring("隐藏窗口").as_ptr(),
    );
    AppendMenuW(
        hmenu,
        MF_STRING,
        CMD_EXIT as usize,
        to_wstring("退出").as_ptr(),
    );

    let mut pos = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pos);

    // 从托盘弹出菜单需要先 SetForegroundWindow，否则菜单不消失
    SetForegroundWindow(hwnd);

    TrackPopupMenu(
        hmenu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        pos.x,
        pos.y,
        0,
        hwnd,
        null_mut(),
    );

    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(hmenu);
}

// ── 事件发送 ──────────────────────────────────────────────────────────────

/// 通过全局 TRAY_TX 向主线程发送事件。
fn send_event(action: TrayAction) {
    if let Some(tx) = TRAY_TX.get() {
        let _ = tx.send(action);
    }
}

// ── 窗口过程 ──────────────────────────────────────────────────────────────

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // 窗口创建 → 添加托盘图标
        WM_CREATE => {
            if !add_tray_icon(hwnd) {
                return -1;
            }
            0
        }

        // 托盘图标回调
        // lParam 携带鼠标消息类型（WM_RBUTTONUP / WM_LBUTTONDBLCLK …）
        WM_TRAYICON => {
            match lparam as UINT {
                WM_RBUTTONUP => show_context_menu(hwnd),
                WM_LBUTTONDBLCLK => {
                    send_event(TrayAction::ShowWindow);
                }
                _ => {}
            }
            0
        }

        // 菜单命令
        WM_COMMAND => {
            match wparam as UINT {
                CMD_SHOW => {
                    send_event(TrayAction::ShowWindow);
                }
                CMD_HIDE => {
                    send_event(TrayAction::HideWindow);
                }
                CMD_EXIT => {
                    send_event(TrayAction::ExitApp);
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }

        // 关闭窗口 → 隐藏（不退出）
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }

        // 窗口销毁 → 清理托盘图标 + 退出消息循环
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            PostQuitMessage(0);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── 后台线程 ──────────────────────────────────────────────────────────────

/// 后台线程入口：注册窗口类 → 创建隐藏窗口 → 添加托盘图标 → 消息循环。
fn tray_thread(_tx: Sender<TrayAction>) {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(null_mut());
        let class_name = to_wstring("FileSeekerTrayWindow");

        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as UINT,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: null_mut(),
        };

        if RegisterClassExW(&wc) == 0 {
            return;
        }

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            to_wstring("文件检索助手").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            400,
            300,
            null_mut(),
            null_mut(),
            hinst,
            null_mut(),
        );

        if hwnd.is_null() {
            return;
        }

        // 窗口保持隐藏 —— 应用只在托盘运行
        ShowWindow(hwnd, SW_HIDE);

        // 标准 Windows 消息循环
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}