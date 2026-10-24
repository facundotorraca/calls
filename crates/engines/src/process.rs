use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{Context, Result, bail};

const TAIL_LINES: usize = 20;

pub enum Stream {
    Stdout,
    Stderr,
}

/// Runs `cmd` calling `on_line` for each line the tool prints on `stream`, feeding it `input`
/// on stdin if given. The other stream is drained in the background. On failure, the last
/// lines of both end up in the error.
pub fn run_lines(
    mut cmd: Command,
    input: Option<Vec<u8>>,
    stream: Stream,
    mut on_line: impl FnMut(&str),
) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not start {program}"))?;

    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        // From a thread: a tool may start printing before it has read all its input.
        thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
    }

    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let (watched, other): (Box<dyn Read + Send>, Box<dyn Read + Send>) = match stream {
        Stream::Stdout => (Box::new(stdout), Box::new(stderr)),
        Stream::Stderr => (Box::new(stderr), Box::new(stdout)),
    };

    let other_tail = thread::spawn(move || tail(other, |_| {}));
    let watched_tail = tail(watched, &mut on_line);
    let other_tail = other_tail.join().unwrap_or_default();

    let status = child.wait()?;
    if !status.success() {
        let output: Vec<String> = other_tail.into_iter().chain(watched_tail).collect();
        bail!("{program} failed ({status}):\n{}", output.join("\n"));
    }
    Ok(())
}

fn tail(reader: impl Read, mut on_line: impl FnMut(&str)) -> VecDeque<String> {
    let mut last = VecDeque::with_capacity(TAIL_LINES);
    for line in BufReader::new(reader).lines().map_while(Result::ok) {
        on_line(&line);
        if last.len() == TAIL_LINES {
            last.pop_front();
        }
        last.push_back(line);
    }
    last
}
