#![cfg(unix)]

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct PtyStep {
    pub await_text: &'static str,
    pub send: &'static str,
}

pub struct PtyOutput {
    pub status: Option<ExitStatus>,
    pub output: String,
}

pub fn run_pty(mut command: Command, steps: &[PtyStep]) -> PtyOutput {
    let (master, slave) = open_pty();
    let slave = unsafe { File::from_raw_fd(slave) };
    command
        .stdin(Stdio::from(slave.try_clone().expect("clone pty slave")))
        .stdout(Stdio::from(slave.try_clone().expect("clone pty slave")))
        .stderr(Stdio::from(slave));

    let mut child = command.spawn().expect("spawn command on a pty");
    drop(command);
    let mut reader = unsafe { File::from_raw_fd(master) };
    let writer_fd = unsafe { libc::dup(master) };
    assert!(writer_fd >= 0, "dup pty master");
    let mut writer = unsafe { File::from_raw_fd(writer_fd) };

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let reader_buffer = Arc::clone(&buffer);
    let reader_thread = thread::spawn(move || {
        let mut chunk = [0u8; 1024];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => reader_buffer
                    .lock()
                    .expect("pty buffer lock")
                    .extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });

    let mut cursor = 0;
    for step in steps {
        cursor = wait_for(&buffer, step.await_text, cursor, Duration::from_secs(10));
        writer
            .write_all(step.send.as_bytes())
            .expect("write pty input");
        writer.flush().expect("flush pty input");
    }

    let status = wait_for_exit(&mut child, Duration::from_secs(10));
    drop(writer);
    reader_thread.join().expect("join pty reader");
    let output = String::from_utf8_lossy(&buffer.lock().expect("pty buffer lock")).into_owned();

    PtyOutput { status, output }
}

fn wait_for(buffer: &Arc<Mutex<Vec<u8>>>, needle: &str, start: usize, timeout: Duration) -> usize {
    let deadline = Instant::now() + timeout;
    loop {
        let snapshot = buffer.lock().expect("pty buffer lock").clone();
        let text = String::from_utf8_lossy(&snapshot);
        if let Some(index) = text[start..].find(needle) {
            return start + index + needle.len();
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {needle:?}; output so far: {text:?}");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn wait_for_exit(child: &mut std::process::Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("poll pty child") {
            return Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn open_pty() -> (RawFd, RawFd) {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let mut window = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(window),
        )
    };
    assert_eq!(result, 0, "openpty failed: {}", io::Error::last_os_error());
    (master, slave)
}
