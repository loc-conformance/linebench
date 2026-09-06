use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use colored::{ColoredString, Colorize};

pub use linebench::measure::print_line;
use linebench::measure::strip_ansi;

pub struct Output {
    transcript: Option<File>,
    printed_before: Vec<u8>,
}

impl Output {
    pub fn new() -> Output {
        Output {
            transcript: None,
            printed_before: Vec::new(),
        }
    }

    pub fn start_transcript(&mut self, path: &Path) -> Result<(), String> {
        let mut file = File::create(path)
            .map_err(|error| format!("{} could not be created: {error}", path.display()))?;
        let _ = file.write_all(&self.printed_before);
        self.printed_before.clear();
        self.transcript = Some(file);
        Ok(())
    }

    pub fn write_to_transcript(&mut self, line: &str) {
        if let Some(file) = &mut self.transcript {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn stop_transcript(&mut self) {
        self.transcript = None;
    }
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let _ = io::stdout().write_all(bytes);
        let plain = strip_ansi(&String::from_utf8_lossy(bytes));
        match &mut self.transcript {
            Some(file) => {
                let _ = file.write_all(plain.as_bytes());
            }
            None => self.printed_before.extend_from_slice(plain.as_bytes()),
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stdout().flush();
        if let Some(file) = &mut self.transcript {
            let _ = file.flush();
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Bold,
    Green,
    Red,
    Yellow,
    Orange,
    Blue,
}

pub fn enable_colors() {
    #[cfg(windows)]
    let _ = colored::control::set_virtual_terminal(true);
}

pub fn print_header(out: &mut dyn Write, title: &str) -> Result<(), String> {
    print_line(out, "")?;
    print_line(out, &paint(Color::Bold, title).to_string())
}

pub fn print_warning(out: &mut dyn Write, message: &str) -> Result<(), String> {
    print_line(
        out,
        &format!("{}{message}", paint(Color::Yellow, "WARNING: ")),
    )
}

pub fn paint(color: Color, text: &str) -> ColoredString {
    match color {
        Color::Bold => text.bold(),
        Color::Green => text.green(),
        Color::Red => text.red(),
        Color::Yellow => text.yellow(),
        Color::Orange => text.truecolor(220, 140, 60),
        Color::Blue => text.truecolor(110, 160, 220),
    }
}
