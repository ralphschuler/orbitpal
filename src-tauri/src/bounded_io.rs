//! Limits are enforced before copying external bytes into growing buffers.
use std::{future::Future, process::Output};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt},
    process::Child,
    task::JoinHandle,
};

pub struct ByteBudget {
    remaining: usize,
    label: &'static str,
}
impl ByteBudget {
    pub fn new(limit: usize, label: &'static str) -> Self { Self { remaining: limit, label } }
    pub fn remaining(&self) -> usize { self.remaining }
    pub fn consume(&mut self, size: usize) -> Result<(), String> {
        self.remaining = self.remaining.checked_sub(size)
            .ok_or_else(|| format!("{} überschreitet die Größenbegrenzung", self.label))?;
        Ok(())
    }
}

pub async fn read(reader: impl AsyncRead + Unpin, limit: usize, label: &'static str) -> Result<Vec<u8>, String> {
    let mut reader = reader;
    let mut budget = ByteBudget::new(limit, label);
    let mut result = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        // One extra byte distinguishes an exact-limit EOF from overflow without
        // reading an arbitrarily large line or body first.
        let capacity = chunk.len().min(budget.remaining().saturating_add(1));
        let size = reader.read(&mut chunk[..capacity]).await.map_err(|e| format!("{label}: {e}"))?;
        if size == 0 { return Ok(result); }
        budget.consume(size)?;
        result.extend_from_slice(&chunk[..size]);
    }
}

pub async fn line(reader: &mut (impl AsyncBufRead + Unpin), budget: &mut ByteBudget) -> Result<Option<Vec<u8>>, String> {
    line_into(reader, budget, &mut Vec::new()).await
}

/// Keep a partial line in the transport across cancellation. A later request's
/// fresh budget also accounts for that retained prefix before accepting bytes.
pub async fn line_into(reader: &mut (impl AsyncBufRead + Unpin), budget: &mut ByteBudget, result: &mut Vec<u8>) -> Result<Option<Vec<u8>>, String> {
    budget.consume(result.len())?;
    loop {
        let available = reader.fill_buf().await.map_err(|e| format!("{}: {e}", budget.label))?;
        if available.is_empty() { return Ok((!result.is_empty()).then(|| std::mem::take(result))); }
        let end = available.iter().position(|byte| *byte == b'\n').map(|index| index + 1);
        let size = end.unwrap_or(available.len());
        budget.consume(size)?;
        result.extend_from_slice(&available[..size]);
        reader.consume(size);
        if end.is_some() { return Ok(Some(std::mem::take(result))); }
    }
}

/// Drain both pipes concurrently. On overflow/read failure kill and reap the
/// child; callers retain their existing timeout/cancellation and kill_on_drop.
pub async fn capture_child(mut child: Child, stdout_limit: usize, stderr_limit: usize, label: &'static str) -> Result<Output, String> {
    let stdout = child.stdout.take().ok_or_else(|| format!("{label}: stdout fehlt"))?;
    let stderr = child.stderr.take().ok_or_else(|| format!("{label}: stderr fehlt"))?;
    let result = tokio::try_join!(
        read(stdout, stdout_limit, label),
        read(stderr, stderr_limit, label),
        async { child.wait().await.map_err(|e| format!("{label}: {e}")) },
    );
    match result {
        Ok((stdout, stderr, status)) => Ok(Output { status, stdout, stderr }),
        Err(error) => { let _ = child.kill().await; Err(error) }
    }
}

/// A long-lived RPC worker must observe stderr overflow even when stdout is
/// silent. Normal stderr EOF is harmless; dropping the worker aborts the drain.
pub struct Stderr {
    task: JoinHandle<Result<Vec<u8>, String>>,
    complete: bool,
}
impl Stderr {
    pub fn new(reader: impl AsyncRead + Unpin + Send + 'static, limit: usize, label: &'static str) -> Self {
        Self { task: tokio::spawn(read(reader, limit, label)), complete: false }
    }
    pub async fn watch<T>(&mut self, future: impl Future<Output = Result<T, String>>) -> Result<T, String> {
        tokio::pin!(future);
        loop {
            tokio::select! {
                biased;
                result = &mut self.task, if !self.complete => {
                    self.complete = true;
                    result.map_err(|_| "CLI-Fehlerausgabe konnte nicht überwacht werden".to_owned())??;
                },
                result = &mut future => return result,
            }
        }
    }
}
impl Drop for Stderr { fn drop(&mut self) { self.task.abort(); } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::{pin::Pin, sync::{Arc, atomic::{AtomicUsize, Ordering}}, task::{Context, Poll}};
    use tokio::io::{BufReader, ReadBuf};

    struct Endless(Arc<AtomicUsize>);
    impl AsyncRead for Endless {
        fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
            let count = buffer.remaining();
            buffer.put_slice(&vec![b'x'; count]);
            self.0.fetch_add(count, Ordering::SeqCst);
            Poll::Ready(Ok(()))
        }
    }
    #[tokio::test]
    async fn body_accepts_exact_limit_and_reads_only_one_excess_byte() {
        assert_eq!(read(&b"1234"[..], 4, "fixture").await.unwrap(), b"1234");
        let bytes = Arc::new(AtomicUsize::new(0));
        assert!(read(Endless(bytes.clone()), 17, "fixture").await.unwrap_err().contains("Größenbegrenzung"));
        assert_eq!(bytes.load(Ordering::SeqCst), 18);
    }
    #[tokio::test]
    async fn incomplete_lines_and_aggregate_lines_are_bounded() {
        let bytes = Arc::new(AtomicUsize::new(0));
        let mut reader = BufReader::with_capacity(8, Endless(bytes.clone()));
        assert!(line(&mut reader, &mut ByteBudget::new(17, "fixture")).await.is_err());
        assert_eq!(bytes.load(Ordering::SeqCst), 24); // Fixed BufReader prefetch only.
        let mut reader = BufReader::new(&b"123\n45\n"[..]);
        let mut budget = ByteBudget::new(6, "fixture");
        assert_eq!(line(&mut reader, &mut budget).await.unwrap().unwrap(), b"123\n");
        assert!(line(&mut reader, &mut budget).await.is_err());
        let mut exact = BufReader::new(&b"123\n"[..]);
        let mut budget = ByteBudget::new(4, "fixture");
        assert!(line(&mut exact, &mut budget).await.unwrap().is_some());
        assert!(line(&mut exact, &mut budget).await.unwrap().is_none());
    }
    #[tokio::test]
    async fn stderr_overflow_interrupts_a_silent_rpc_without_hanging() {
        let mut stderr = Stderr::new(Endless(Arc::new(AtomicUsize::new(0))), 32, "CLI stderr");
        let error = tokio::time::timeout(std::time::Duration::from_secs(1), stderr.watch(std::future::pending::<Result<(), String>>())).await.unwrap().unwrap_err();
        assert!(error.contains("CLI stderr"));
        assert!(error.contains("Größenbegrenzung"));
    }
    #[tokio::test]
    async fn cancelled_stdio_read_retains_and_recounts_its_partial_line() {
        use tokio::io::AsyncWriteExt;
        let (mut writer, reader) = tokio::io::duplex(32);
        let mut reader = BufReader::new(reader);
        writer.write_all(b"part").await.unwrap();
        let mut pending = Vec::new();
        let result = tokio::time::timeout(std::time::Duration::from_millis(10), line_into(&mut reader, &mut ByteBudget::new(8, "fixture"), &mut pending)).await;
        assert!(result.is_err());
        assert_eq!(pending, b"part");
        assert!(line_into(&mut reader, &mut ByteBudget::new(3, "fixture"), &mut pending).await.is_err());
        writer.write_all(b"ial\n").await.unwrap();
        assert_eq!(line_into(&mut reader, &mut ByteBudget::new(8, "fixture"), &mut pending).await.unwrap().unwrap(), b"partial\n");
        assert!(pending.is_empty());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn overflowing_process_is_killed_and_reaped() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "while :; do printf '1234567890'; done"])
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        let child = command.spawn().unwrap();
        let pid = child.id().unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), capture_child(child, 32, 32, "fixture")).await.unwrap();
        assert!(result.unwrap_err().contains("Größenbegrenzung"));
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
        assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }
}
