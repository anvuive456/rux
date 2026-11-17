//! The dev server the app's hot reload client connects to (see `RuxDevClient` in the native
//! runtimes for the protocol).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

pub struct DevServer {
    listener: TcpListener,
    /// The app's latest connection (it reconnects if its process restarts).
    conn: Arc<Mutex<Option<BufReader<TcpStream>>>>,
}

impl DevServer {
    /// Listens on a free localhost port.
    pub fn bind() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").context("starting the dev server")?;
        Ok(Self {
            listener,
            conn: Arc::default(),
        })
    }

    pub fn port(&self) -> u16 {
        self.listener.local_addr().map(|a| a.port()).unwrap_or(0)
    }

    /// Accepts app connections in the background, keeping the latest.
    pub fn accept_in_background(&self) -> Result<()> {
        let listener = self.listener.try_clone()?;
        let conn = self.conn.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
                *conn.lock().unwrap() = Some(BufReader::new(stream));
            }
        });
        Ok(())
    }

    /// Waits until the app has connected.
    pub fn wait_for_app(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while self.conn.lock().unwrap().is_none() {
            if Instant::now() > deadline {
                bail!(
                    "the app did not connect to the dev server within {}s",
                    timeout.as_secs()
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    /// Sends a library build (`command` is `reload` or `restart`) and returns the app's report.
    pub fn send(&self, command: &str, library: &[u8]) -> Result<String> {
        let mut guard = self.conn.lock().unwrap();
        let conn = guard.as_mut().context("the app is not connected")?;
        let result = (|| -> std::io::Result<String> {
            let stream = conn.get_mut();
            stream.write_all(format!("{command} {}\n", library.len()).as_bytes())?;
            stream.write_all(library)?;
            stream.flush()?;
            let mut report = String::new();
            if conn.read_line(&mut report)? == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            Ok(report.trim_end().to_owned())
        })();
        if result.is_err() {
            *guard = None; // dead connection; wait for the app to reconnect
        }
        result.context("the app disconnected")
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    use super::DevServer;

    #[test]
    fn sends_builds_and_reads_reports() {
        let server = DevServer::bind().unwrap();
        server.accept_in_background().unwrap();
        let port = server.port();

        // A fake app: reads one build, answers with its size.
        let app = std::thread::spawn(move || {
            let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            let (command, size) = header.trim().split_once(' ').unwrap();
            let mut bytes = vec![0; size.parse().unwrap()];
            reader.read_exact(&mut bytes).unwrap();
            writeln!(&stream, "ok {command} {}", bytes.len()).unwrap();
        });

        server.wait_for_app(Duration::from_secs(5)).unwrap();
        assert_eq!(server.send("reload", b"12345").unwrap(), "ok reload 5");
        app.join().unwrap();
        assert!(server.send("reload", b"x").is_err()); // app gone
    }
}
