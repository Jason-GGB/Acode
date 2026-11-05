use crate::runtime::terminal::prepare_command;
use std::process::{Child, Command, Stdio};

/// PowerShell on Windows, login zsh on macOS and other Unix.
pub(crate) fn spawn_shell(command: &str, cwd: Option<&std::path::Path>) -> std::io::Result<Child> {
    let mut cmd = shell_command(command);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.spawn()
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("powershell");
        crate::runtime::terminal::prepare_powershell(&mut cmd, command);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("/bin/zsh");
        cmd.args(["-lc", command]);
        cmd
    }
}

/// Kills a process and its entire child tree.
pub fn terminate_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut command = Command::new("taskkill");
        command
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        prepare_command(&mut command);
        let _ = command.status();
    }
    let _ = child.kill();
}
