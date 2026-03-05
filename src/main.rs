use std::io::{self, BufRead, Write};

fn main() {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // send the Assuan greeting
    writeln!(out, "OK Pleased to meet you").expect("failed to write greeting");
    out.flush().expect("failed to flush stdout");

    // read one command and acknowledge it, then exit
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    if lines.next().is_some() {
        writeln!(out, "OK").expect("failed to write OK");
        out.flush().expect("failed to flush stdout");
    }
}
