use std::io::{self, IsTerminal, Write};

#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

pub fn is_tty() -> bool {
    io::stdin().is_terminal()
}

pub fn prompt_hidden(prompt: &str) -> io::Result<Option<String>> {
    let echo = EchoGuard::disable();
    {
        let mut stderr = io::stderr().lock();
        write!(stderr, "{prompt}")?;
        stderr.flush()?;
    }
    let line = read_line();
    drop(echo);
    {
        let mut stderr = io::stderr().lock();
        writeln!(stderr)?;
        stderr.flush()?;
    }
    line
}

pub fn confirm(prompt: &str) -> io::Result<Option<bool>> {
    {
        let mut stderr = io::stderr().lock();
        write!(stderr, "{prompt}")?;
        stderr.flush()?;
    }
    let answer = read_line()?;
    Ok(answer.map(|answer| matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")))
}

fn read_line() -> io::Result<Option<String>> {
    let mut line = String::new();
    let read = io::stdin().read_line(&mut line)?;
    if read == 0 {
        Ok(None)
    } else {
        Ok(Some(line.trim_end_matches(['\r', '\n']).to_string()))
    }
}

struct EchoGuard {
    #[cfg(unix)]
    active: bool,
}

impl EchoGuard {
    fn disable() -> Self {
        #[cfg(unix)]
        {
            let active = stty("-echo")
                .map(|status| status.success())
                .unwrap_or(false);
            Self { active }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }
}

impl Drop for EchoGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if self.active {
            let _ = stty("echo");
        }
    }
}

#[cfg(unix)]
fn stty(argument: &str) -> std::io::Result<std::process::ExitStatus> {
    for path in ["/bin/stty", "/usr/bin/stty"] {
        if Path::new(path).is_file() {
            return Command::new(path).arg(argument).status();
        }
    }
    Command::new("stty").arg(argument).status()
}
