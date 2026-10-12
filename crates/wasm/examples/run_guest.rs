use efi_agent_wasm::clock::{Clock, ClockError, ClockId};
use efi_agent_wasm::{Status, World};
#[path = "support/native_fs.rs"]
mod native_fs;
#[path = "support/native_net.rs"]
mod native_net;
use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
    rc::Rc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct HostClock(Instant);
struct HostRandom;
impl efi_agent_wasm::random::Random for HostRandom {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), efi_agent_wasm::random::RandomError> {
        getrandom::getrandom(bytes).map_err(|_| efi_agent_wasm::random::RandomError::Io)
    }
}
impl Clock for HostClock {
    fn now(&self, id: ClockId) -> Result<u64, ClockError> {
        let duration = match id {
            ClockId::Realtime => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| ClockError::Io)?,
            ClockId::Monotonic => self.0.elapsed(),
            _ => return Err(ClockError::Unavailable),
        };
        duration
            .as_nanos()
            .try_into()
            .map_err(|_| ClockError::Overflow)
    }
    fn resolution(&self, id: ClockId) -> Result<u64, ClockError> {
        match id {
            ClockId::Realtime | ClockId::Monotonic => Ok(1_000_000),
            _ => Err(ClockError::Unavailable),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args().skip(1);
    let directory = PathBuf::from(arguments.next().ok_or("Expected program directory")?);
    let argv: Vec<String> = arguments.collect();
    let mut world = World::new();
    world.set_clock(Rc::new(HostClock(Instant::now())));
    world.set_random(Rc::new(HostRandom));
    world.set_network(Rc::new(native_net::NativeNetwork));
    let dns_server: std::net::SocketAddrV4 = env::var("EFI_WASM_DNS")
        .unwrap_or_else(|_| "1.1.1.1:53".into())
        .parse()?;
    world.set_dns_server(dns_server.ip().octets(), dns_server.port());
    world.set_filesystem(Rc::new(native_fs::NativeFileSystem(directory.clone())));
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "wasm")
        {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or("Invalid program name")?;
            world.register(name, &fs::read(&path)?)?;
        }
    }
    let root = world.spawn(argv, Vec::new())?;
    let deadline = Instant::now() + std::time::Duration::from_secs(60);
    while Instant::now() < deadline {
        if world.status(root)?.is_live() && world.step().is_none() {
            // Timed poll can leave the entire world asleep until a host clock
            // reaches its deadline. Give the host a bounded idle interval.
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let (stdout, stderr) = world.take_output(root)?;
        io::stdout().write_all(&stdout)?;
        io::stderr().write_all(&stderr)?;
        match world.status(root)? {
            Status::Exited(0) => return Ok(()),
            Status::Exited(code) => return Err(format!("Guest exited with {code}").into()),
            Status::Trapped(error) => return Err(format!("Guest trapped: {error}").into()),
            _ => {}
        }
    }
    Err("Guest did not exit within 60 seconds".into())
}
