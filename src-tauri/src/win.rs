use std::process::Command;

// 子プロセス起動時にコンソールウィンドウを瞬間表示させないための flag。
// Windows GUI アプリ (= Tauri 本体は windows_subsystem = "windows" で隠してる) から
// console subsystem の child を spawn すると、 default で window がフラッシュする。
// CREATE_NO_WINDOW (= 0x0800_0000) を creation_flags に渡すと抑止される。
// 非 Windows では no-op。
#[cfg(windows)]
pub fn hide_window(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
pub fn hide_window(cmd: &mut Command) -> &mut Command {
    cmd
}
