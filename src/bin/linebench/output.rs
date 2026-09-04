use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use colored::{ColoredString, Colorize};

use linebench::measure::strip_ansi;

pub struct Output {
    transcript: Option<File>,
}

impl Output {
    pub fn new() -> Output {
        Output { transcript: None }
    }

    pub fn start_transcript(&mut self, path: &Path) -> Result<(), String> {
        let file = File::create(path)
            .map_err(|error| format!("{} could not be created: {error}", path.display()))?;
        self.transcript = Some(file);
        Ok(())
    }

    pub fn stop_transcript(&mut self) {
        self.transcript = None;
    }
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        io::stdout().write_all(bytes)?;
        if let Some(file) = &mut self.transcript {
            let plain = strip_ansi(&String::from_utf8_lossy(bytes));
            let _ = file.write_all(plain.as_bytes());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stdout().flush()?;
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

pub fn print_line(out: &mut dyn Write, message: &str) -> Result<(), String> {
    writeln!(out, "{message}")
        .map_err(|error| format!("this report could not be written: {error}"))?;
    out.flush()
        .map_err(|error| format!("this report could not be written: {error}"))
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
