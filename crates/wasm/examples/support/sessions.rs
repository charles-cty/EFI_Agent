//! Native integration driver for the same retained guest session protocol.
use efi_agent_wasm::{
    Status, World,
    session::{Session, SessionKind},
};
use std::{
    fs,
    io::{self, Write},
    time::{Duration, Instant},
};

type Error = Box<dyn std::error::Error>;

fn stop(world: &mut World, root: u32, keep_root: bool) -> Result<(), Error> {
    let children: Vec<_> = world
        .process_ids()
        .filter(|pid| *pid != root && world.root(*pid).unwrap() == root)
        .collect();
    if !keep_root {
        world.cancel(root)?;
    }
    for &pid in &children {
        world.cancel(pid)?;
    }
    let (stdout, stderr) = world.take_output(root)?;
    io::stdout().write_all(&stdout)?;
    io::stderr().write_all(&stderr)?;
    for pid in children {
        world.reap(pid)?;
    }
    if !keep_root {
        world.reap(root)?;
    }
    Ok(())
}

pub fn run(world: &mut World, requests: &[String]) -> Result<(), Error> {
    if !requests.len().is_multiple_of(2) {
        return Err("Sessions require operation/source-file pairs".into());
    }
    let mut sessions: [Option<Session>; 2] = [None, None];
    for (index, request) in requests.chunks_exact(2).enumerate() {
        if request[0] == "reset_js" {
            if let Some(session) = sessions[SessionKind::JavaScript.slot()].take() {
                stop(world, session.pid(), false)?;
            }
            println!("SESSION_JS_RESET {index}");
            continue;
        }
        if request[0] == "clear" {
            for session in &mut sessions {
                if let Some(session) = session.take() {
                    stop(world, session.pid(), false)?;
                }
            }
            println!("SESSION_CLEAR {index}");
            continue;
        }
        let (kind, cancel) = match request[0].as_str() {
            "shell" => (SessionKind::Shell, false),
            "js" => (SessionKind::JavaScript, false),
            "cancel-shell" => (SessionKind::Shell, true),
            "cancel-js" => (SessionKind::JavaScript, true),
            _ => return Err("Unknown session operation".into()),
        };
        let slot = kind.slot();
        if sessions[slot].is_none() {
            sessions[slot] = Some(world.start_session(kind)?);
        }
        let session = sessions[slot].as_mut().unwrap();
        session.submit(&fs::read_to_string(&request[1])?)?;
        let pid = session.pid();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut seen = Vec::new();
        let (code, reset) = loop {
            if Instant::now() >= deadline {
                return Err("Session request timed out".into());
            }
            if world.step().is_none() {
                std::thread::sleep(Duration::from_millis(1));
            }
            let (stdout, stderr) = world.take_output(pid)?;
            seen.extend_from_slice(&stdout);
            io::stdout().write_all(&stdout)?;
            io::stderr().write_all(&stderr)?;
            if cancel
                && seen
                    .windows(b"SESSION_CANCEL_READY".len())
                    .any(|window| window == b"SESSION_CANCEL_READY")
            {
                world.cancel(pid)?;
            }
            if let Some(code) = session.take_completion() {
                break (code, false);
            }
            match world.status(pid)? {
                Status::Exited(code) => break (*code, true),
                Status::Trapped(error) => return Err(format!("Session trapped: {error}").into()),
                _ => {}
            }
        };
        stop(world, pid, !reset)?;
        if reset {
            sessions[slot] = None;
        }
        println!("SESSION_RESULT {index} pid={pid} code={code} reset={reset}");
    }
    for session in sessions.into_iter().flatten() {
        stop(world, session.pid(), false)?;
    }
    if world.process_ids().next().is_some() {
        return Err("Session driver left an unreaped process".into());
    }
    Ok(())
}
