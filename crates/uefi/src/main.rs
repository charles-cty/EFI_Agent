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
mod capabilities;
mod dns;
mod drivers;
mod environment;
mod event_loop;
mod files;
mod tcp;
mod terminal;
mod tls;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    if let Err(error) = boot::set_watchdog_timer(0, 0x10000, None)
        && error.status() != Status::UNSUPPORTED
    {
        uefi::println!("Cannot disable UEFI watchdog: {error}");
        return error.status();
    }
    // Firmware boot managers need not connect optional device drivers before
    // starting an application. Do this before opening long-lived protocols.
    let driver_report = drivers::initialize();
    let vm = files::read("\\EFI\\AGENT\\VM.TXT").is_ok();
    match run(&driver_report) {
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
    runtime_state: &mut Option<environment::Runtime>,
    text: String,
    driver_report: &str,
    mut refresh: impl FnMut(&mut App, bool) -> Result<bool, terminal::Error>,
) -> Result<(), terminal::Error> {
    if matches!(text.as_str(), "/quit" | "/exit") {
        app.quit = true;
        return Ok(());
    }
    app.message("user", text.clone());
    if text == "/clear" {
        agent.clear();
        app.clear_messages();
        app.scroll = 0;
        app.status = String::from("Conversation cleared");
        return Ok(());
    }
    let previous_status = app.status.clone();
    app.status = String::from("Working");
    refresh(app, false)?;
    let response = if text == "/caps" {
        app.capabilities = capabilities::detect(driver_report);
        app.capabilities.clone()
    } else if text == "/help" {
        String::from(
            "/help  Show commands\n/caps  Probe firmware network and cryptographic RNG capabilities\n/effort [value]  Show or set reasoning effort (provider validates values)\n/status  Show actual API, history, token and cache data\n/clear  Start a new conversation\n/quit, /exit  Exit\nSend a prompt to run the coding agent (read, write, edit tools).",
        )
    } else if text == "/status" {
        let history = agent.history_status().unwrap_or_else(|error| error);
        let provider = runtime_state.as_mut().map_or_else(
            || String::from("API: unavailable (model environment not configured)"),
            |runtime| {
                runtime
                    .control(efi_agent_core::protocol::Operation::Status)
                    .unwrap_or_else(|error| alloc::format!("API status unavailable: {error}"))
            },
        );
        alloc::format!("State before command: {previous_status}\n{history}\n{provider}")
    } else if text.split_whitespace().next() == Some("/effort") {
        let mut arguments = text.split_whitespace().skip(1);
        let value = arguments.next().map(String::from);
        if arguments.next().is_some() {
            String::from("Usage: /effort [value]")
        } else if let Some(runtime) = runtime_state.as_mut() {
            runtime
                .control(efi_agent_core::protocol::Operation::Effort { value })
                .unwrap_or_else(|error| error)
        } else {
            String::from("Reasoning effort unavailable: model environment not configured")
        }
    } else if text.starts_with('/') {
        String::from("Unknown command. Use /help.")
    } else if let Some(runtime_state) = runtime_state.as_mut() {
        app.status = String::from("Requesting model");
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
                runtime: runtime_state,
                poll: &mut control,
                cancelled: false,
            };
            let mut streaming_row = None;
            let mut tool_row = None;
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
                    Event::Reasoning { text, summary } => app.detail(
                        if summary {
                            "reasoning_summary"
                        } else {
                            "reasoning"
                        },
                        String::from(if summary {
                            "Reasoning summary"
                        } else {
                            "Reasoning (provider text)"
                        }),
                        text.into(),
                    ),
                    Event::ToolStarted { name, arguments } => {
                        app.status = alloc::format!("Running {name}");
                        tool_row = Some(app.start_tool(name, arguments));
                    }
                    Event::ToolFinished {
                        name,
                        result,
                        failed,
                    } => {
                        app.status = String::from("Waiting for model");
                        app.finish_tool(
                            tool_row.take().expect("Tool result follows its call"),
                            name,
                            result,
                            failed,
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
    } else {
        String::from(
            "Model environment is not configured. Provide EFI/AGENT/CONFIG.JSON with API credentials and a boot-volume workspace.",
        )
    };
    app.message("assistant", response);
    app.status = String::from("Ready");
    refresh(app, false)?;
    Ok(())
}

/// Keep editing responsive during requests; defer only submission.
fn request_key(app: &mut App, deferred: &mut VecDeque<Key>, key: Key) -> bool {
    match key {
        Key::Escape => true,
        Key::Quit => {
            app.quit = true;
            true
        }
        Key::Enter if app.editor.text.is_empty() => {
            // An empty editor uses Enter to toggle focused disclosure panels.
            app.key(Key::Enter);
            false
        }
        Key::Enter => {
            // Bound buffered input to the same order of size as a prompt.
            if deferred.len() < 65536 {
                deferred.push_back(key);
            }
            false
        }
        key => {
            app.key(key);
            false
        }
    }
}

fn draw_serial(
    terminal: &mut Terminal<Ansi<terminal::SerialSink>>,
    app: &mut App,
) -> Result<(), terminal::Error> {
    terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
    let view = serde_json::to_vec(&app.selection_view)
        .map_err(|_| terminal::Error(Status::OUT_OF_RESOURCES))?;
    let sink = &mut terminal.backend_mut().sink;
    sink.write(efi_agent_core::serial::VIEW_PREFIX)?;
    sink.write(&view)?;
    sink.write(b"\x07")
}

fn run(driver_report: &str) -> Result<(), terminal::Error> {
    let mut app = App::default();
    let mut agent = Agent::default();
    app.capabilities = capabilities::detect(driver_report);
    let vm = files::read("\\EFI\\AGENT\\VM.TXT").is_ok();
    let mut runtime_state = match environment::Runtime::load(vm) {
        Ok(environment) => Some(environment),
        Err(error) => {
            app.status = alloc::format!("Configuration: {error}");
            None
        }
    };
    if let Some(runtime) = &runtime_state {
        app.workspace = runtime.config.workspace.clone();
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
                draw_serial(&mut terminal, &mut app)?;
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
                        submit(
                            &mut app,
                            &mut agent,
                            &mut runtime_state,
                            text,
                            driver_report,
                            |app, poll| {
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
                                    draw_serial(&mut terminal, app)?;
                                }
                                Ok(stop)
                            },
                        )?;
                    }
                }
                event_loop::idle(core::time::Duration::from_millis(10))
                    .map_err(|_| terminal::Error(Status::DEVICE_ERROR))?;
            }
            terminal.show_cursor()?;
            return Ok(());
        }
    }
    let mut terminal = Terminal::new(terminal::SimpleText::new()?)?;
    let mut pointer = terminal::ConsolePointer::new();
    let mut deferred = VecDeque::new();
    terminal.clear()?;
    terminal.hide_cursor()?;
    while !app.quit {
        terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
        if let Some(key) = terminal.backend().read_key() {
            deferred.push_back(key);
        }
        if let Some(key) = pointer.poll(terminal.size()?) {
            deferred.push_back(key);
        }
        while !app.quit
            && let Some(key) = deferred.pop_front()
        {
            if let Some(text) = app.key(key) {
                submit(
                    &mut app,
                    &mut agent,
                    &mut runtime_state,
                    text,
                    driver_report,
                    |app, poll| {
                        let mut stop = false;
                        let mut changed = !poll;
                        if poll && let Some(key) = terminal.backend().read_key() {
                            stop = request_key(app, &mut deferred, key);
                            changed = true;
                        }
                        if poll && let Some(key) = pointer.poll(terminal.size()?) {
                            stop |= request_key(app, &mut deferred, key);
                            changed = true;
                        }
                        if changed {
                            terminal.draw(|frame| app.render(frame.area(), frame.buffer_mut()))?;
                        }
                        Ok(stop)
                    },
                )?;
            }
        }
        event_loop::idle(core::time::Duration::from_millis(10))
            .map_err(|_| terminal::Error(Status::DEVICE_ERROR))?;
    }
    terminal.show_cursor()?;
    Ok(())
}
