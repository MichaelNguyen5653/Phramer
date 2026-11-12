//! The answer to `phramer-cli`: what 15.0.0 printed to the console that ran
//! it (`--raw`'s PNG, `--print-geometry`'s line, its log lines) and its
//! exit code.
//!
//! `phramer-cli` creates a named pipe and passes its name along with the
//! command, as one more argument; the daemon connects as soon as the
//! command reaches it and writes frames until the capture is done. A frame
//! is a tag byte, a little-endian `u32` length and that many bytes.

use std::fs::File;
use std::io::{self, Read, Write};

/// The argument carrying the pipe's name. Internal: never typed by a user,
/// stripped before parsing.
pub(crate) const REPLY_ARGUMENT: &str = "--phramer-reply-pipe=";
/// Every reply pipe's name starts so; nothing else is ever opened.
pub(crate) const PIPE_PREFIX: &str = r"\\.\pipe\phramer-cli-";

/// `ErrCode` of 15.0.0's `flameshot.h`.
pub(crate) const EXIT_OK: i32 = 0;
pub(crate) const EXIT_GENERAL: i32 = 1;
pub(crate) const EXIT_ABORTED: i32 = 2;

const TAG_STDOUT: u8 = b'o';
const TAG_STDERR: u8 = b'e';
const TAG_EXIT: u8 = b'x';
/// Large output (a 4K PNG) goes in pieces of this size.
const CHUNK: usize = 64 * 1024;

/// One frame of the reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Frame {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(i32),
}

fn write_frame(out: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    let length = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    out.write_all(&[tag])?;
    out.write_all(&length.to_le_bytes())?;
    out.write_all(payload)
}

/// Writes `frame`, splitting output into chunks.
pub(crate) fn encode(out: &mut impl Write, frame: &Frame) -> io::Result<()> {
    match frame {
        Frame::Stdout(bytes) | Frame::Stderr(bytes) => {
            let tag = if matches!(frame, Frame::Stdout(_)) {
                TAG_STDOUT
            } else {
                TAG_STDERR
            };
            for chunk in bytes.chunks(CHUNK) {
                write_frame(out, tag, chunk)?;
            }
            Ok(())
        }
        Frame::Exit(code) => write_frame(out, TAG_EXIT, &code.to_le_bytes()),
    }
}

/// The next frame; `None` at the end of the stream.
pub(crate) fn decode(input: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut tag = [0u8; 1];
    match input.read_exact(&mut tag) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let mut length = [0u8; 4];
    input.read_exact(&mut length)?;
    let mut payload = vec![0u8; u32::from_le_bytes(length) as usize];
    input.read_exact(&mut payload)?;
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "malformed reply frame");
    Ok(Some(match tag[0] {
        TAG_STDOUT => Frame::Stdout(payload),
        TAG_STDERR => Frame::Stderr(payload),
        TAG_EXIT => Frame::Exit(i32::from_le_bytes(
            payload.as_slice().try_into().map_err(|_| invalid())?,
        )),
        _ => return Err(invalid()),
    }))
}

/// Removes every reply argument from `args`; the pipe name of the last one
/// that names a reply pipe.
pub(crate) fn take_reply(args: &mut Vec<String>) -> Option<String> {
    let mut name = None;
    args.retain(|arg| match arg.strip_prefix(REPLY_ARGUMENT) {
        Some(pipe) => {
            if pipe.starts_with(PIPE_PREFIX) && pipe.len() > PIPE_PREFIX.len() {
                name = Some(pipe.to_owned());
            }
            false
        }
        None => true,
    });
    name
}

/// The daemon's end: the console of a `phramer-cli` waiting for a capture.
/// Dropped without [`Reply::finish`], it reports the capture aborted, as
/// 15.0.0's `captureFailed` did.
#[derive(Debug)]
pub(crate) struct Reply {
    pipe: Option<File>,
}

impl Reply {
    /// Connects to the pipe `phramer-cli` is waiting on.
    pub(crate) fn connect(name: &str) -> Option<Self> {
        match File::options().write(true).open(name) {
            Ok(pipe) => Some(Self { pipe: Some(pipe) }),
            Err(error) => {
                log::warn!("cannot answer phramer-cli on {name}: {error}");
                None
            }
        }
    }

    fn send(&mut self, frame: &Frame) {
        if let Some(pipe) = self.pipe.as_mut()
            && let Err(error) = encode(pipe, frame)
        {
            log::warn!("phramer-cli went away: {error}");
            self.pipe = None;
        }
    }

    pub(crate) fn stdout(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.send(&Frame::Stdout(bytes.to_vec()));
        }
    }

    /// A log line as 15.0.0 printed it: `phramer: <level>: <text>`.
    pub(crate) fn log_line(&mut self, level: &str, text: &str) {
        self.send(&Frame::Stderr(
            format!("phramer: {level}: {text}\n").into_bytes(),
        ));
    }

    /// Ends the reply with `code`.
    pub(crate) fn finish(mut self, code: i32) {
        self.send(&Frame::Exit(code));
        self.pipe = None;
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        if self.pipe.is_some() {
            self.log_line("info", "Screenshot aborted.");
            self.send(&Frame::Exit(EXIT_ABORTED));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip_and_large_output_is_chunked() {
        let big = vec![7u8; CHUNK * 2 + 5];
        let mut bytes = Vec::new();
        encode(&mut bytes, &Frame::Stdout(big.clone())).unwrap();
        encode(&mut bytes, &Frame::Stderr(b"phramer: info: x\n".to_vec())).unwrap();
        encode(&mut bytes, &Frame::Exit(EXIT_ABORTED)).unwrap();
        let mut input = bytes.as_slice();
        let mut out = Vec::new();
        let mut frames = 0;
        let mut rest = Vec::new();
        while let Some(frame) = decode(&mut input).unwrap() {
            frames += 1;
            match frame {
                Frame::Stdout(chunk) => out.extend(chunk),
                other => rest.push(other),
            }
        }
        assert_eq!(out, big);
        assert_eq!(frames, 5);
        assert_eq!(
            rest,
            [
                Frame::Stderr(b"phramer: info: x\n".to_vec()),
                Frame::Exit(2)
            ]
        );
    }

    #[test]
    fn truncated_and_unknown_frames_are_errors() {
        let mut bytes = Vec::new();
        encode(&mut bytes, &Frame::Exit(0)).unwrap();
        let mut truncated = &bytes[..bytes.len() - 1];
        assert!(decode(&mut truncated).is_err());
        let mut unknown: &[u8] = &[b'?', 0, 0, 0, 0];
        assert!(decode(&mut unknown).is_err());
        let mut empty: &[u8] = &[];
        assert_eq!(decode(&mut empty).unwrap(), None);
    }

    #[test]
    fn the_reply_argument_is_taken_out_wherever_it_is() {
        let mut args: Vec<String> = vec![
            "phramer.exe".into(),
            "gui".into(),
            r"--phramer-reply-pipe=\\.\pipe\phramer-cli-12-34".into(),
            "--raw".into(),
        ];
        assert_eq!(
            take_reply(&mut args).as_deref(),
            Some(r"\\.\pipe\phramer-cli-12-34")
        );
        assert_eq!(args, ["phramer.exe", "gui", "--raw"]);
    }

    #[test]
    fn only_reply_pipes_are_accepted() {
        for bad in [
            r"--phramer-reply-pipe=\\.\pipe\other",
            r"--phramer-reply-pipe=C:\file.txt",
            r"--phramer-reply-pipe=\\.\pipe\phramer-cli-",
        ] {
            let mut args = vec!["phramer.exe".to_owned(), bad.to_owned()];
            assert_eq!(take_reply(&mut args), None, "{bad}");
            assert_eq!(args, ["phramer.exe"]);
        }
    }
}
