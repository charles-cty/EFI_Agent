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
mod bridge;
mod files;
mod tcp;
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

fn submit(app: &mut App, bridge: &mut Option<bridge::Bridge>, text: String) {
    if text == "/quit" {
        app.quit = true;
        return;
    }
    app.message("user", text.clone());
    let response = if text == "/help" {
        String::from(
            "/help  Show commands\n/quit  Exit to firmware\n/read <path>  Read a UTF-8 file from the boot volume\n/write <path> <text>  Save a file on the boot volume\n/host-list [path]  List host files\n/host-read <path>  Read a host file\n/host-write <path> <text>  Save a host file\nSend a prompt to query the model configured on the host.",
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
    } else if let Some(bridge) = bridge.as_mut() {
        use efi_agent_core::protocol::Operation;
        let operation = if text == "/host-list" {
            Operation::List {
                path: String::from("."),
            }
        } else if let Some(path) = text.strip_prefix("/host-list ") {
            Operation::List { path: path.into() }
        } else if let Some(path) = text.strip_prefix("/host-read ") {
            Operation::Read { path: path.into() }
        } else if let Some(arguments) = text.strip_prefix("/host-write ") {
            match arguments.split_once(' ') {
                Some((path, content)) => Operation::Write {
                    path: path.into(),
                    content: content.into(),
                },
                None => {
                    app.message(
                        "assistant",
                        String::from("Usage: /host-write <path> <text>"),
                    );
                    return;
                }
            }
        } else {
            Operation::Complete {
                messages: app.messages.clone(),
            }
        };
        bridge.call(operation).unwrap_or_else(|e| e)
    } else {
        String::from(
            "HostBridge is not configured. Boot in VM mode to use host files and the model.",
        )
    };
    app.message("assistant", response);
}

fn run() -> Result<(), terminal::Error> {
    let mut app = App::default();
    let vm = files::read("\\EFI\\AGENT\\VM.TXT").is_ok();
    let mut bridge = if vm { Some(bridge::Bridge::vm()) } else { None };
    // VM mode is explicit. The launcher ESP has no motherboard UART enabled.
    if vm && let Ok(handles) = boot::find_handles::<Serial>() {
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
                let count = terminal.backend_mut().sink.read(&mut bytes)?;
                for byte in &bytes[..count] {
                    if let Some(key) = decoder.push(*byte) {
                        if let Key::Resize(w, h) = key {
                            terminal.backend_mut().dimensions = Size::new(w.min(300), h.min(120));
                            terminal.autoresize()?;
                        } else if let Some(text) = app.key(key) {
                            submit(&mut app, &mut bridge, text);
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
            submit(&mut app, &mut bridge, text);
        }
        boot::stall(core::time::Duration::from_millis(10));
    }
    terminal.show_cursor()?;
    Ok(())
}
