//! The Assuan server side of the pinentry protocol.

mod handlers;
mod protocol;
mod session;

pub use protocol::{ERR_NO_PIN_ENTRY, write_err};
pub use session::{Handoff, Session, Step};

/// A session connected to gpg-agent over stdin and stdout.
pub type StdioSession = Session<std::io::BufReader<std::io::Stdin>, std::io::Stdout>;
