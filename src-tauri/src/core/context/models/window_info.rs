/// 前台窗口信息 — 由 WindowDetector 填充。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub hwnd: isize,
    pub pid: u32,
    pub process_name: String,
    pub title: String,
}

impl WindowInfo {
    pub fn is_explorer(&self) -> bool {
        let name = self.process_name.to_ascii_lowercase();
        name == "explorer.exe" || name == "finder"
    }

    /// Terminal / console hosts where Ctrl+C means SIGINT, not “copy”.
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        let name = self.process_name.to_ascii_lowercase();
        matches!(
            name.as_str(),
            "windowsterminal.exe"
                | "cmd.exe"
                | "powershell.exe"
                | "pwsh.exe"
                | "conhost.exe"
                | "bash.exe"
                | "wsl.exe"
                | "ubuntu.exe"
                | "debian.exe"
                | "openssh.exe"
                | "ssh.exe"
                | "mintty.exe"
                | "alacritty.exe"
                | "alacritty"
                | "wezterm-gui.exe"
                | "wezterm.exe"
                | "wezterm"
                | "kitty.exe"
                | "kitty"
                | "hyper.exe"
                | "hyper"
                | "tabby.exe"
                | "tabby"
                | "fluentterminal.exe"
                | "windowsterminalpreview.exe"
                | "terminal"
                | "iterm2"
                | "warp"
                | "ghostty"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_terminals() {
        let wt = WindowInfo {
            hwnd: 0,
            pid: 1,
            process_name: "WindowsTerminal.exe".into(),
            title: "Terminal".into(),
        };
        assert!(wt.is_terminal());
        let notepad = WindowInfo {
            hwnd: 0,
            pid: 1,
            process_name: "notepad.exe".into(),
            title: "notes".into(),
        };
        assert!(!notepad.is_terminal());
        let finder = WindowInfo {
            hwnd: 0,
            pid: 1,
            process_name: "Finder".into(),
            title: "Desktop".into(),
        };
        assert!(finder.is_explorer());
        let terminal = WindowInfo {
            hwnd: 0,
            pid: 1,
            process_name: "Terminal".into(),
            title: "shell".into(),
        };
        assert!(terminal.is_terminal());
    }
}
