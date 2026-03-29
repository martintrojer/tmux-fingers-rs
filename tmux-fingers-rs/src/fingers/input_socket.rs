use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InputSocket {
    path: PathBuf,
}

impl InputSocket {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn on_input<F>(&self, mut handler: F) -> std::io::Result<()>
    where
        F: FnMut(String) -> bool,
    {
        remove_socket_file(&self.path)?;
        let listener = UnixListener::bind(&self.path)?;

        for stream in listener.incoming() {
            let stream = stream?;
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line)?;
            let message = line.trim_end_matches('\n').to_string();
            if !handler(message) {
                break;
            }
        }

        remove_socket_file(&self.path)?;
        Ok(())
    }

    pub fn send_message(&self, cmd: &str) -> std::io::Result<()> {
        let mut socket = UnixStream::connect(&self.path)?;
        socket.write_all(cmd.as_bytes())?;
        socket.write_all(b"\n")?;
        socket.flush()
    }
}

fn remove_socket_file(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::InputSocket;

    #[test]
    fn sends_and_receives_messages() {
        let socket_path =
            std::env::temp_dir().join(format!("tmux-fingers-rs-{}.sock", std::process::id()));
        let listener = InputSocket::new(socket_path.clone());
        let sender = listener.clone();
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            listener
                .on_input(|msg| {
                    tx.send(msg).unwrap();
                    false
                })
                .unwrap();
        });

        thread::sleep(Duration::from_millis(100));
        sender.send_message("hey").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), "hey");
        handle.join().unwrap();
    }
}
