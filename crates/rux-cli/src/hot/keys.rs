//! The session's commands, read from the terminal: single keys (no Enter) when stdin is a
//! terminal, else one command per line (e.g. stdin piped from a script).

use std::io::Read;
use std::sync::mpsc::Sender;

use super::Input;

/// Stdin switched to single-key input; restores the terminal when dropped.
pub struct Keys {
    #[cfg(unix)]
    saved: Option<libc::termios>,
}

impl Keys {
    /// Starts reading commands into `tx` on a background thread.
    pub fn read(tx: Sender<Input>) -> Self {
        let keys = Self {
            #[cfg(unix)]
            saved: unix::single_keys(),
        };
        let single = keys.single();
        std::thread::spawn(move || {
            if single {
                read_bytes(&tx)
            } else {
                read_lines(&tx)
            }
        });
        keys
    }

    /// Whether a command is a single key, or a line (key + Enter).
    pub fn single(&self) -> bool {
        #[cfg(unix)]
        return self.saved.is_some();
        #[cfg(not(unix))]
        return false;
    }

    /// The key hint shown in the help, e.g. `r` or `r ⏎`.
    pub fn hint(&self, key: char) -> String {
        if self.single() {
            key.to_string()
        } else {
            format!("{key} ⏎")
        }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(saved) = &self.saved {
            unix::restore(saved);
        }
    }
}

fn read_bytes(tx: &Sender<Input>) {
    let mut stdin = std::io::stdin().lock();
    let mut byte = [0u8; 1];
    while let Ok(1) = stdin.read(&mut byte) {
        if let Some(key) = key(byte[0])
            && tx.send(Input::Key(key)).is_err()
        {
            break;
        }
    }
}

fn read_lines(tx: &Sender<Input>) {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else { break };
        let Some(first) = line.trim().bytes().next() else {
            continue;
        };
        if let Some(key) = key(first)
            && tx.send(Input::Key(key)).is_err()
        {
            break;
        }
    }
}

/// Maps a byte typed by the user to a command key: Ctrl-C / Ctrl-D quit (signals are off in
/// single-key mode), other control bytes (escape sequences, …) are ignored.
fn key(byte: u8) -> Option<char> {
    match byte {
        0x03 | 0x04 => Some('q'),
        b if b.is_ascii_graphic() => Some(b as char),
        _ => None,
    }
}

#[cfg(unix)]
mod unix {
    /// Turns off line buffering, echo and signal keys on stdin if it's a terminal; returns the
    /// previous settings. Output processing is left as is, so the output of builds and logs
    /// prints normally.
    pub fn single_keys() -> Option<libc::termios> {
        // SAFETY: termios calls on fd 0 with a properly sized, initialized struct.
        unsafe {
            if libc::isatty(libc::STDIN_FILENO) != 1 {
                return None;
            }
            let mut termios: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) != 0 {
                return None;
            }
            let saved = termios;
            termios.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
            termios.c_cc[libc::VMIN] = 1;
            termios.c_cc[libc::VTIME] = 0;
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &termios) != 0 {
                return None;
            }
            Some(saved)
        }
    }

    pub fn restore(saved: &libc::termios) {
        // SAFETY: restores settings previously read from the same fd.
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, saved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::key;

    #[test]
    fn maps_bytes_to_keys() {
        assert_eq!(key(b'r'), Some('r'));
        assert_eq!(key(b'R'), Some('R'));
        assert_eq!(key(0x03), Some('q')); // Ctrl-C
        assert_eq!(key(0x04), Some('q')); // Ctrl-D
        assert_eq!(key(0x1b), None); // escape (arrow keys, …)
        assert_eq!(key(b'\n'), None);
    }
}
