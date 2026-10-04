#![no_std]
#![no_main]

extern crate alloc;

use alloc::{collections::VecDeque, string::String};
use core::cell::{Cell, RefCell};
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
mod capabilities;
mod environment;
mod event_loop;
mod files;
mod tcp;
mod terminal;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    if let Err(error) = boot::set_watchdog_timer(0, 0x10000, None)
        && error.status() != Status::UNSUPPORTED
    {
        uefi::println!("Cannot disable UEFI watchdog: {error}");
        return error.status();
    }
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
    bridge: &mut Option<environment::Runtime>,
    text: String,
    mut refresh: impl FnMut(&mut App, bool) -> Result<bool, terminal::Error>,
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
    refresh(app, false)?;
    let response = if text == "/capabilities" {
        app.capabilities = capabilities::detect();
        app.capabilities.clone()
    } else if text == "/help" {
        String::from(
            "/help  Show commands\n/capabilities  Probe firmware network and cryptographic RNG capabilities\n/clear  Start a new conversation\n/quit  Exit (VM: shut down; hardware: return to firmware)\n/read <path>  Read a UTF-8 file from the boot volume\n/write <path> <text>  Save a file on the boot volume\n/host-list [path]  List host files\n/host-read <path>  Read a host file\n/host-write <path> <text>  Save a host file\nSend a prompt to run the coding agent (read, write, edit tools).",
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
            refresh(app, false)?;
            let render_error = RefCell::new(None);
            let cancelled = Cell::new(false);
            let result = {
                let ui = RefCell::new((&mut *app, &mut refresh));
                let mut control = || {
                    if cancelled.get() {
                        return true;
                    }
                    let mut ui = ui.borrow_mut();
                    let (app, refresh) = &mut *ui;
                    match refresh(app, true) {
                        Ok(stop) => cancelled.set(stop),
                        Err(error) => {
                            *render_error.borrow_mut() = Some(error);
                            cancelled.set(true);
                        }
                    }
                    cancelled.get()
                };
                let mut environment = environment::Interactive {
                    runtime: bridge,
                    poll: &mut control,
                    cancelled: false,
                };
                let mut streaming_row = None;
                agent.turn(text, &mut environment, |event| {
                    let mut ui = ui.borrow_mut();
                    let (app, refresh) = &mut *ui;
                    match event {
                        Event::ModelStarted => {
                            streaming_row = None;
                            app.status = String::from("Waiting for model");
                        }
                        Event::AssistantDelta(content) => {
                            let index = *streaming_row.get_or_insert_with(|| {
                                app.message("assistant", String::new());
                                app.messages.len() - 1
                            });
                            app.messages[index].content.push_str(content);
                            app.status = String::from("Receiving model response");
                        }
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
                    let healthy = render_error.borrow().is_none();
                    if healthy && let Err(error) = refresh(app, false) {
                        *render_error.borrow_mut() = Some(error);
                        cancelled.set(true);
                    }
                })
            };
            if let Err(error) = result {
                app.message("error", error);
            }
            app.status = String::from("Ready");
            if let Some(error) = render_error.into_inner() {
                return Err(error);
            }
            refresh(app, false)?;
            return Ok(());
        };
        {
            let mut render_error = None;
            let result = bridge.host(operation, &mut || match refresh(app, true) {
                Ok(stop) => stop,
                Err(error) => {
                    render_error = Some(error);
                    true
                }
            });
            if let Some(error) = render_error {
                return Err(error);
            }
            result.unwrap_or_else(|e| e)
        }
    } else {
        String::from(
            "Model environment is not configured. On hardware, provide EFI/AGENT/NATIVE.JSON with a model relay and native workspace.",
        )
    };
    app.message("assistant", response);
    app.status = String::from("Ready");
    refresh(app, false)?;
    Ok(())
}

/// Apply request controls now; retain other input for the normal editor loop.
fn request_key(app: &mut App, deferred: &mut VecDeque<Key>, key: Key) -> bool {
    match key {
        Key::Escape => true,
        Key::Quit => {
            app.quit = true;
            true
        }
        Key::Up | Key::Down => {
            app.key(key);
            false
        }
        _ => {
            // Bound buffered input to the same order of size as a prompt.
            if deferred.len() < 65536 {
                deferred.push_back(key);
            }
            false
        }
    }
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
    app.capabilities = capabilities::detect();
    let vm = files::read("\\EFI\\AGENT\\VM.TXT").is_ok();
    let mut bridge = match environment::Runtime::load(vm) {
        Ok(environment) => Some(environment),
        Err(error) => {
            app.status = alloc::format!("Configuration: {error}");
            None
        }
    };
    if let Some(environment::Runtime::Native { config, .. }) = &bridge {
        app.workspace = config.workspace.clone();
    }
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
            let mut deferred = VecDeque::new();
            // Resolve the host dimensions before drawing the first frame.
            // A direct serial monitor can omit the reply and use the fallback.
            let mut sized = false;
            for _ in 0..200 {
                let mut bytes = [0; 128];
                let count = terminal.backend_mut().sink.read(&mut bytes)?;
                if count == 0 {
                    decoder.idle();
                }
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
                event_loop::idle(core::time::Duration::from_millis(10))
                    .map_err(|_| terminal::Error(Status::DEVICE_ERROR))?;
            }
            while !app.quit {
                terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
                let mut bytes = [0; 128];
                let count = terminal.backend_mut().sink.read(&mut bytes)?;
                if count == 0
                    && let Some(key) = decoder.idle()
                {
                    deferred.push_back(key);
                }
                for byte in &bytes[..count] {
                    if let Some(key) = decoder.push(*byte) {
                        deferred.push_back(key);
                    }
                }
                while !app.quit
                    && let Some(key) = deferred.pop_front()
                {
                    if let Key::Resize(w, h) = key {
                        terminal.backend_mut().dimensions = Size::new(w.min(300), h.min(120));
                        terminal.autoresize()?;
                    } else if let Some(text) = app.key(key) {
                        submit(&mut app, &mut agent, &mut bridge, text, |app, poll| {
                            let mut stop = false;
                            let mut changed = !poll;
                            if poll {
                                let mut bytes = [0; 128];
                                let count = terminal.backend_mut().sink.read(&mut bytes)?;
                                if count == 0
                                    && let Some(key) = decoder.idle()
                                {
                                    stop |= request_key(app, &mut deferred, key);
                                    changed = true;
                                }
                                for byte in &bytes[..count] {
                                    if let Some(key) = decoder.push(*byte) {
                                        if let Key::Resize(w, h) = key {
                                            terminal.backend_mut().dimensions =
                                                Size::new(w.min(300), h.min(120));
                                            terminal.autoresize()?;
                                        } else {
                                            stop |= request_key(app, &mut deferred, key);
                                        }
                                        changed = true;
                                    }
                                }
                            }
                            if changed {
                                terminal
                                    .draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
                            }
                            Ok(stop)
                        })?;
                    }
                }
                event_loop::idle(core::time::Duration::from_millis(10))
                    .map_err(|_| terminal::Error(Status::DEVICE_ERROR))?;
            }
            terminal.show_cursor()?;
            return Ok(());
        }
    }
    let mut terminal = Terminal::new(terminal::SimpleText)?;
    let mut deferred = VecDeque::new();
    terminal.clear()?;
    terminal.hide_cursor()?;
    while !app.quit {
        terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
        if let Some(key) = terminal::console_key() {
            deferred.push_back(key);
        }
        while !app.quit
            && let Some(key) = deferred.pop_front()
        {
            if let Some(text) = app.key(key) {
                submit(&mut app, &mut agent, &mut bridge, text, |app, poll| {
                    let mut stop = false;
                    let mut changed = !poll;
                    if poll && let Some(key) = terminal::console_key() {
                        stop = request_key(app, &mut deferred, key);
                        changed = true;
                    }
                    if changed {
                        terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
                    }
                    Ok(stop)
                })?;
            }
        }
        event_loop::idle(core::time::Duration::from_millis(10))
            .map_err(|_| terminal::Error(Status::DEVICE_ERROR))?;
    }
    terminal.show_cursor()?;
    Ok(())
}
