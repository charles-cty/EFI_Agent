use efi_agent_wasm::{
    Status, World,
    clock::{Clock, ClockError, ClockId},
    filesystem::{FileSystem, OpenOptions, namespace::Namespace},
    session::SessionKind,
};
use std::{rc::Rc, time::Instant};

struct HostClock(Instant);
impl Clock for HostClock {
    fn now(&self, _: ClockId) -> Result<u64, ClockError> {
        Ok(self.0.elapsed().as_nanos() as u64)
    }
    fn resolution(&self, _: ClockId) -> Result<u64, ClockError> {
        Ok(1_000_000)
    }
}

#[test]
#[ignore = "requires a built BusyBox image in EFI_AGENT_ASH"]
fn real_pager_uses_the_controlling_terminal_for_file_and_pipe_input() {
    let mut world = World::new();
    world.set_clock(Rc::new(HostClock(Instant::now())));
    let filesystem = Rc::new(Namespace::temporary(1024 * 1024, 128, 1, None));
    let mut file = filesystem
        .open(
            "pages",
            OpenOptions {
                read: true,
                write: true,
                create: true,
                exclusive: true,
                truncate: false,
                append: false,
            },
        )
        .unwrap();
    file.write(
        &(1..=80)
            .map(|line| format!("PAGE_{line:03}\n"))
            .collect::<String>()
            .into_bytes(),
    )
    .unwrap();
    world.set_filesystem(filesystem).unwrap();
    world
        .register(
            "ash",
            &std::fs::read(std::env::var_os("EFI_AGENT_ASH").unwrap()).unwrap(),
        )
        .unwrap();
    let mut session = world.start_session(SessionKind::Shell).unwrap();
    for source in [
        "exec 9</dev/tty; test -t 9 || exit 71; less /pages",
        "cat /pages | less",
    ] {
        session.submit_terminal(source).unwrap();
        let mut output = Vec::new();
        for _ in 0..2000 {
            world.step();
            output.extend(session.take_terminal_output());
            if session.terminal_mode() & 1 == 0 {
                break;
            }
            assert!(
                !String::from_utf8_lossy(&output).contains("PAGE_001"),
                "Pager drew its first page before entering the terminal view"
            );
            if let Some(status) = session.take_completion() {
                panic!(
                    "Pager exited before raw mode: {status}; {}",
                    String::from_utf8_lossy(&output)
                );
            }
            assert!(
                world.status(session.pid()).unwrap().is_live(),
                "{}",
                String::from_utf8_lossy(&output)
            );
        }
        assert_eq!(
            session.terminal_mode() & 1,
            0,
            "{}",
            String::from_utf8_lossy(&output)
        );
        session.write_stdin(b" q").unwrap();
        let mut result = None;
        for _ in 0..2000 {
            world.step();
            output.extend(session.take_terminal_output());
            if let Some(status) = session.take_completion() {
                result = Some(status);
                break;
            }
        }
        assert_eq!(result, Some(0), "{}", String::from_utf8_lossy(&output));
        let output = String::from_utf8_lossy(&output);
        assert!(
            output.contains("PAGE_001") && output.contains("PAGE_030"),
            "{output}"
        );
        assert_eq!(session.terminal_mode(), 7);
        assert!(matches!(
            world.status(session.pid()).unwrap(),
            Status::Reading(_)
        ));
        session.detach_terminal();
    }
}
