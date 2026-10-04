#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use efi_agent_core::{
    ansi::Ansi,
    input::{Decoder, Key},
    ui::App,
};
use ratatui::{Terminal, layout::Size};
use uefi::prelude::*;
use uefi::{boot, proto::console::serial::Serial};
mod files;
mod terminal;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    match run() {
        Ok(()) => Status::SUCCESS,
        Err(error) => {
            uefi::println!("{error}");
            Status::DEVICE_ERROR
        }
    }
}

fn submit(app: &mut App, text: String) {
    if text == "/quit" {
        app.quit = true;
        return;
    }
    app.message("user", text.clone());
    let response = if text == "/help" {
        String::from(
            "/help  Show commands\n/quit  Exit to firmware\n/read <path>  Read a UTF-8 file from the boot volume\n/write <path> <text>  Save a file on the boot volume\nHostBridge and model connection are not yet enabled.",
        )
    } else if let Some(path) = text.strip_prefix("/read ") {
        files::read(path).unwrap_or_else(|e| e)
    } else if let Some(arguments) = text.strip_prefix("/write ") {
        match arguments.split_once(' ') {
            Some((path, content)) => files::write(path, content)
                .map(|()| String::from("File saved"))
                .unwrap_or_else(|e| e),
            None => String::from("Usage: /write <path> <text>"),
        }
    } else {
        String::from("Model connection is not yet enabled.")
    };
    app.message("assistant", response);
}

fn run() -> Result<(), terminal::Error> {
    let mut app = App::default();
    // VM mode is explicit. The launcher ESP has no motherboard UART enabled.
    if files::read("\\EFI\\AGENT\\VM.TXT").is_ok()
        && let Ok(handles) = boot::find_handles::<Serial>()
    {
        for handle in handles {
            let Ok(mut serial) = boot::open_protocol_exclusive::<Serial>(handle) else {
                continue;
            };
            let mut mode = *serial.io_mode();
            mode.timeout = 1000;
            serial
                .set_attributes(&mode)
                .map_err(|e| terminal::Error(e.status()))?;
            let mut terminal =
                Terminal::new(Ansi::new(terminal::SerialSink(serial), Size::new(100, 30)))?;
            terminal.hide_cursor()?;
            let mut decoder = Decoder::default();
            while !app.quit {
                terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
                let mut bytes = [0; 128];
                let count = match terminal.backend_mut().sink.0.read(&mut bytes) {
                    Ok(()) => bytes.len(),
                    Err(e) if e.status() == Status::TIMEOUT => *e.data(),
                    Err(e) => return Err(terminal::Error(e.status())),
                };
                for byte in &bytes[..count] {
                    if let Some(key) = decoder.push(*byte) {
                        if let Key::Resize(w, h) = key {
                            terminal.backend_mut().dimensions = Size::new(w.min(300), h.min(120));
                            terminal.autoresize()?;
                        } else if let Some(text) = app.key(key) {
                            submit(&mut app, text);
                        }
                    }
                }
                boot::stall(core::time::Duration::from_millis(10));
            }
            terminal.show_cursor()?;
            return Ok(());
        }
    }
    let mut terminal = Terminal::new(terminal::SimpleText)?;
    terminal.clear()?;
    terminal.hide_cursor()?;
    while !app.quit {
        terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
        if let Some(key) = terminal::console_key()
            && let Some(text) = app.key(key)
        {
            submit(&mut app, text);
        }
        boot::stall(core::time::Duration::from_millis(10));
    }
    terminal.show_cursor()?;
    Ok(())
}
