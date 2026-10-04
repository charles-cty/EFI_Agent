#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use efi_agent_core::{
    agent::{Agent, Event},
    ansi::Ansi,
    ansi::Sink,
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
    let vm = files::read("\\EFI\\AGENT\\VM.TXT").is_ok();
    match run() {
        Ok(()) => {
            if vm {
                uefi::runtime::reset(uefi::runtime::ResetType::SHUTDOWN, Status::SUCCESS, None);
            }
            Status::SUCCESS
        }
        Err(error) => {
            uefi::println!("{error}");
            Status::DEVICE_ERROR
        }
    }
}

fn submit(
    app: &mut App,
    agent: &mut Agent,
    bridge: &mut Option<bridge::Bridge>,
    text: String,
    mut render: impl FnMut(&App) -> Result<(), terminal::Error>,
) -> Result<(), terminal::Error> {
    if text == "/quit" {
        app.quit = true;
        return Ok(());
    }
    app.message("user", text.clone());
    if text == "/clear" {
        agent.clear();
        app.messages.clear();
        app.scroll = 0;
        app.status = String::from("Conversation cleared");
        return Ok(());
    }
    app.status = String::from("Working");
    render(app)?;
    let response = if text == "/help" {
        String::from(
            "/help  Show commands\n/clear  Start a new conversation\n/quit  Exit (VM: shut down; hardware: return to firmware)\n/read <path>  Read a UTF-8 file from the boot volume\n/write <path> <text>  Save a file on the boot volume\n/host-list [path]  List host files\n/host-read <path>  Read a host file\n/host-write <path> <text>  Save a host file\nSend a prompt to run the coding agent (read, write, edit tools).",
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
                    app.status = String::from("Ready");
                    return Ok(());
                }
            }
        } else if text.starts_with('/') {
            app.message("assistant", String::from("Unknown command. Use /help."));
            app.status = String::from("Ready");
            return Ok(());
        } else {
            app.status = String::from("Waiting for model");
            render(app)?;
            let mut render_error = None;
            let result = agent.turn(text, bridge, |event| {
                match event {
                    Event::Assistant(content) => app.message("assistant", content.into()),
                    Event::ToolStarted { name, arguments } => {
                        app.status = alloc::format!("Running {name}");
                        app.message("tool", alloc::format!("{name} {}", preview(arguments)))
                    }
                    Event::ToolFinished {
                        name,
                        result,
                        failed,
                    } => {
                        app.status = String::from("Waiting for model");
                        app.message(
                            "tool",
                            alloc::format!(
                                "{name}: {}\n{}",
                                if failed { "failed" } else { "done" },
                                preview(result)
                            ),
                        );
                    }
                }
                if render_error.is_none() {
                    render_error = render(app).err();
                }
            });
            if let Err(error) = result {
                app.message("error", error);
            }
            app.status = String::from("Ready");
            if let Some(error) = render_error {
                return Err(error);
            }
            return render(app);
        };
        bridge.call(operation).unwrap_or_else(|e| e)
    } else {
        String::from(
            "HostBridge is not configured. Boot in VM mode to use host files and the model.",
        )
    };
    app.message("assistant", response);
    app.status = String::from("Ready");
    render(app)
}

fn preview(text: &str) -> String {
    match text.char_indices().nth(1500) {
        Some((index, _)) => alloc::format!("{}\n[Preview truncated]", &text[..index]),
        None => text.into(),
    }
}

fn run() -> Result<(), terminal::Error> {
    let mut app = App::default();
    let mut agent = Agent::default();
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
            terminal
                .backend_mut()
                .sink
                .write(efi_agent_core::serial::READY)?;
            let mut decoder = Decoder::default();
            // Resolve the host dimensions before drawing the first frame.
            // A direct serial monitor can omit the reply and use the fallback.
            let mut sized = false;
            for _ in 0..200 {
                let mut bytes = [0; 128];
                let count = terminal.backend_mut().sink.read(&mut bytes)?;
                for byte in &bytes[..count] {
                    if let Some(Key::Resize(w, h)) = decoder.push(*byte) {
                        terminal.backend_mut().dimensions = Size::new(w.min(300), h.min(120));
                        terminal.autoresize()?;
                        sized = true;
                    }
                }
                if sized {
                    break;
                }
                boot::stall(core::time::Duration::from_millis(10));
            }
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
                            submit(&mut app, &mut agent, &mut bridge, text, |app| {
                                terminal
                                    .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
                                    .map(|_| ())
                            })?;
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
            submit(&mut app, &mut agent, &mut bridge, text, |app| {
                terminal
                    .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
                    .map(|_| ())
            })?;
        }
        boot::stall(core::time::Duration::from_millis(10));
    }
    terminal.show_cursor()?;
    Ok(())
}
