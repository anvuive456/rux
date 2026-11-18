//! Sends the process's stdout / stderr to logcat (tag `rux`), where `rux run` streams them from.
//! Android discards both by default, which would hide `println!`, `eprintln!` and panics.

/// Redirects stdout / stderr to logcat (once per process; later calls are no-ops).
pub fn redirect_stdio() {
    #[cfg(target_os = "android")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(android::redirect);
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::ffi::{CString, c_char, c_int};
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;

    const ANDROID_LOG_INFO: c_int = 4;

    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    pub fn redirect() {
        let mut fds = [0 as c_int; 2];
        // SAFETY: plain POSIX calls on fds this function owns; stdout / stderr are replaced
        // by the pipe's write end, whose read end is moved into the reader thread.
        let read = unsafe {
            if libc::pipe(fds.as_mut_ptr()) != 0 {
                return;
            }
            libc::dup2(fds[1], libc::STDOUT_FILENO);
            libc::dup2(fds[1], libc::STDERR_FILENO);
            libc::close(fds[1]);
            File::from_raw_fd(fds[0])
        };
        let _ = std::thread::Builder::new()
            .name("rux-logcat".into())
            .spawn(move || {
                let mut reader = BufReader::new(read);
                let mut line = Vec::new();
                while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
                    while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                        line.pop();
                    }
                    line.retain(|b| *b != 0);
                    if let Ok(text) = CString::new(std::mem::take(&mut line)) {
                        // SAFETY: both strings are NUL-terminated and outlive the call.
                        unsafe {
                            __android_log_write(ANDROID_LOG_INFO, c"rux".as_ptr(), text.as_ptr())
                        };
                    }
                }
            });
    }
}
