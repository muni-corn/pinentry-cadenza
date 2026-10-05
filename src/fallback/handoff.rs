//! Continues an in-progress Assuan session in a terminal pinentry.
//!
//! By the time the pinentry knows that the caller is a terminal, it has already
//! greeted gpg-agent, so it can't `exec()` a replacement. Instead it starts the
//! terminal pinentry as a child process, brings it up to date by replaying the
//! commands received so far, and then relays bytes in both directions.

use std::{
    io::{self, BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
};

use crate::assuan::{Handoff, StdioSession};

/// Hands the session over to the first terminal pinentry in `candidates` that
/// can be started, and exits with that program's status.
///
/// Never returns. Prints a diagnostic and exits with code 1 on failure.
pub fn hand_off_to_terminal(session: StdioSession, candidates: &[&str]) -> ! {
    match run(session.into_handoff(), candidates) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("pinentry-cadenza: couldn't hand off to a terminal pinentry: {e}");
            std::process::exit(1);
        }
    }
}

/// Runs the handoff and returns the child's exit code.
fn run<R, W>(handoff: Handoff<R, W>, candidates: &[&str]) -> io::Result<i32>
where
    R: BufRead + Send + 'static,
    W: Write,
{
    let Handoff {
        reader,
        mut writer,
        transcript,
        pending_line,
    } = handoff;

    let mut child = spawn_first(candidates)?;
    let (mut child_in, mut child_out) = take_pipes(&mut child)?;

    expect_ok(&mut child_out)?;
    for line in transcript {
        writeln!(child_in, "{line}")?;
        // the child's answer is discarded: gpg-agent already got ours, and
        // options a terminal pinentry doesn't know are harmless
        read_reply(&mut child_out)?;
    }
    writeln!(child_in, "{pending_line}")?;

    // client to child; this thread ends when the client closes the pipe, and
    // the process exits right after the child does
    thread::spawn(move || {
        let mut reader = reader;
        let _ = io::copy(&mut reader, &mut child_in);
    });

    // child to client, flushed eagerly so prompts and replies aren't delayed
    loop {
        let chunk = child_out.fill_buf()?;
        if chunk.is_empty() {
            break;
        }
        writer.write_all(chunk)?;
        writer.flush()?;
        let len = chunk.len();
        child_out.consume(len);
    }

    Ok(child.wait()?.code().unwrap_or(1))
}

/// Starts the first candidate that exists, with piped stdin and stdout.
fn spawn_first(candidates: &[&str]) -> io::Result<Child> {
    for candidate in candidates {
        match Command::new(candidate)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(child) => return Ok(child),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(io::Error::new(e.kind(), format!("{candidate}: {e}"))),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("none of {} were found on PATH", candidates.join(", ")),
    ))
}

fn take_pipes(child: &mut Child) -> io::Result<(ChildStdin, BufReader<ChildStdout>)> {
    let missing = || io::Error::other("the child's pipes were not created");
    let stdin = child.stdin.take().ok_or_else(missing)?;
    let stdout = child.stdout.take().ok_or_else(missing)?;
    Ok((stdin, BufReader::new(stdout)))
}

/// Reads the child's greeting and checks that it is a success reply.
fn expect_ok(child_out: &mut impl BufRead) -> io::Result<()> {
    let greeting = read_reply(child_out)?;
    if greeting.starts_with("OK") {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "unexpected greeting from the terminal pinentry: {greeting}"
        )))
    }
}

/// Reads lines until a final `OK` or `ERR` reply, and returns that line.
fn read_reply(child_out: &mut impl BufRead) -> io::Result<String> {
    let mut line = String::new();
    loop {
        line.clear();
        if child_out.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the terminal pinentry closed the connection early",
            ));
        }
        let reply = line.trim_end();
        if reply.starts_with("OK") || reply.starts_with("ERR") {
            return Ok(reply.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Cursor, os::unix::fs::PermissionsExt, path::PathBuf, sync::Mutex};

    use super::*;

    // writing and then exec'ing scripts from parallel threads can fail with
    // "text file busy", so the tests that need a script take turns
    static SCRIPT_LOCK: Mutex<()> = Mutex::new(());

    const FAKE_PINENTRY: &str = r#"#!/bin/sh
echo "OK fake ready"
while read -r line; do
  echo "$line" >> "$0.log"
  case "$line" in
    GETPIN) echo "D hunter2"; echo "OK" ;;
    BYE) echo "OK bye"; exit 0 ;;
    *) echo "OK" ;;
  esac
done
"#;

    fn install_script(name: &str, body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_file(format!("{}.log", path.display()));
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn handoff<W>(rest_of_input: &str, writer: W) -> Handoff<Cursor<Vec<u8>>, W> {
        Handoff {
            reader: Cursor::new(rest_of_input.as_bytes().to_vec()),
            writer,
            transcript: vec!["OPTION ttyname=/dev/pts/1".into(), "SETDESC hi".into()],
            pending_line: "GETPIN".into(),
        }
    }

    #[test]
    fn replays_the_session_and_relays_the_conversation() {
        let _guard = SCRIPT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let script = install_script("cadenza-fake-pinentry", FAKE_PINENTRY);

        // run() consumes the writer, so share a buffer to inspect afterwards
        let shared = SharedBuf::default();
        let h = handoff("BYE\n", shared.clone());

        let code = run(h, &[script.to_str().unwrap()]).unwrap();

        assert_eq!(code, 0);
        assert_eq!(shared.contents(), "D hunter2\nOK\nOK bye\n");
        let received = fs::read_to_string(format!("{}.log", script.display())).unwrap();
        assert_eq!(
            received,
            "OPTION ttyname=/dev/pts/1\nSETDESC hi\nGETPIN\nBYE\n"
        );
    }

    #[test]
    fn skips_missing_candidates() {
        let _guard = SCRIPT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let script = install_script("cadenza-fake-pinentry-2", FAKE_PINENTRY);
        let h = handoff("BYE\n", Vec::new());
        let code = run(h, &["/nonexistent/pinentry", script.to_str().unwrap()]).unwrap();
        assert_eq!(code, 0);
    }

    #[test]
    fn reports_when_no_candidate_exists() {
        let err = run(handoff("", Vec::new()), &[
            "/nonexistent/a",
            "/nonexistent/b",
        ])
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains("/nonexistent/a"));
    }

    #[test]
    fn rejects_a_bad_greeting() {
        let _guard = SCRIPT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let script = install_script("cadenza-bad-greeting", "#!/bin/sh\necho 'ERR 1 nope'\n");
        let err = run(handoff("", Vec::new()), &[script.to_str().unwrap()]).unwrap_err();
        assert!(err.to_string().contains("unexpected greeting"));
    }

    /// A cloneable writer that lets a test see what `run` wrote.
    #[derive(Clone, Default)]
    struct SharedBuf(std::sync::Arc<Mutex<Vec<u8>>>);

    impl SharedBuf {
        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
