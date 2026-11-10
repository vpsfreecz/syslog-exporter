use crate::{collectors::Engine, metrics::Diagnostics, parser};
use nix::{
    fcntl::{FcntlArg, fcntl},
    poll::{PollFd, PollFlags, poll},
};
use std::{
    fs::OpenOptions,
    io::{self, Read},
    os::{
        fd::AsFd,
        unix::{
            fs::{FileTypeExt, OpenOptionsExt},
            prelude::MetadataExt,
        },
    },
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// One bounded partial record, plus a discard flag for oversized records.
pub struct Framer {
    buffer: Vec<u8>,
    limit: usize,
    discarding: bool,
}
impl Framer {
    pub fn new(limit: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(limit.min(65536)),
            limit,
            discarding: false,
        }
    }
    /// Feed a span containing at most one LF, at its end.
    pub fn span(&mut self, span: &[u8], engine: &mut Engine, now: Duration) {
        let complete = span.last() == Some(&b'\n');
        if complete {
            engine.diagnostics.records.inc();
        }
        if self.discarding {
            if complete {
                self.discarding = false;
            }
            return;
        }
        if span.len() > self.limit.saturating_sub(self.buffer.len()) {
            self.buffer.clear();
            self.discarding = !complete;
            engine.diagnostics.reject("oversized");
            return;
        }
        if complete && self.buffer.is_empty() {
            Self::record(span, engine, now);
        } else {
            self.buffer.extend_from_slice(span);
            if complete {
                Self::record(&self.buffer, engine, now);
                self.buffer.clear();
            }
        }
    }
    fn record(line: &[u8], engine: &mut Engine, now: Duration) {
        match parser::parse(line) {
            Ok(message) => {
                if matches!(&message.body, std::borrow::Cow::Owned(_)) {
                    engine.diagnostics.body_decoded.inc();
                }
                engine.process(&message, now);
            }
            Err(reason) => engine.diagnostics.reject(reason),
        }
    }
    pub fn end(&mut self, diagnostics: &Diagnostics) {
        if !self.buffer.is_empty() {
            diagnostics.reject("incomplete");
        }
        self.buffer.clear();
        self.discarding = false;
    }
}
fn pause(stop: &AtomicBool) {
    for _ in 0..10 {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
}
pub fn run(
    path: &Path,
    size: i32,
    limit: usize,
    mut engine: Engine,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    let start = Instant::now();
    let mut sweep = start + Duration::from_secs(5);
    let mut framer = Framer::new(limit);
    let mut chunk = [0; 65536];
    let mut opened = false;
    let mut warned_capacity = false;
    while !stop.load(Ordering::Relaxed) {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOFOLLOW)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.file_type().is_fifo() {
            return Err(io::Error::other("syslog_pipe is not a FIFO"));
        }
        // Verify the opened descriptor, not only a path susceptible to replacement.
        if metadata.ino() != std::fs::symlink_metadata(path)?.ino() {
            return Err(io::Error::other("syslog_pipe changed while opening"));
        }
        if opened {
            engine.diagnostics.reconnects.inc();
        }
        opened = true;
        if let Err(error) = fcntl(&file, FcntlArg::F_SETPIPE_SZ(size))
            && !warned_capacity
        {
            eprintln!("unable to set FIFO capacity to {size}: {error}; using kernel capacity");
            warned_capacity = true;
        }
        let capacity = fcntl(&file, FcntlArg::F_GETPIPE_SZ)?;
        engine.diagnostics.capacity.set(i64::from(capacity));
        loop {
            if Instant::now() >= sweep {
                engine.settle(start.elapsed());
                sweep = Instant::now() + Duration::from_secs(5);
            }
            if stop.load(Ordering::Relaxed) {
                framer.end(&engine.diagnostics);
                return Ok(());
            }
            let mut descriptors = [PollFd::new(file.as_fd(), PollFlags::POLLIN)];
            match poll(&mut descriptors, 100u16) {
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(error) => return Err(error.into()),
            }
            match file.read(&mut chunk) {
                Ok(0) => {
                    framer.end(&engine.diagnostics);
                    break;
                }
                Ok(count) => {
                    engine.diagnostics.bytes.inc_by(count as u64);
                    let mut batch = Instant::now();
                    let mut records = 0;
                    for span in chunk[..count].split_inclusive(|byte| *byte == b'\n') {
                        framer.span(span, &mut engine, start.elapsed());
                        if span.last() == Some(&b'\n') {
                            records += 1;
                        }
                        if records >= 256 || batch.elapsed() >= Duration::from_millis(2) {
                            if Instant::now() >= sweep {
                                engine.settle(start.elapsed());
                                sweep = Instant::now() + Duration::from_secs(5);
                            }
                            // Finish all already-read complete records on shutdown; no further read follows.
                            records = 0;
                            batch = Instant::now();
                        }
                    }
                }
                Err(error)
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        drop(file);
        // Also service expiry during repeated no-writer opens.
        if Instant::now() >= sweep {
            engine.settle(start.elapsed());
            sweep = Instant::now() + Duration::from_secs(5);
        }
        pause(&stop);
    }
    framer.end(&engine.diagnostics);
    Ok(())
}
