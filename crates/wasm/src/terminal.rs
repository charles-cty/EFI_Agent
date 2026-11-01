//! Controlling-terminal attributes shared by a direct session and its children.
use crate::{Context, abi, installation::HostLinker, io::Descriptor};
use alloc::rc::Rc;
use wasmi::Caller;

fn check(caller: &Caller<'_, Context>, fd: u32) -> Result<(), i32> {
    let context = caller.data();
    let descriptor = context.descriptors.get(&fd).ok_or(-9)?;
    let matches = match descriptor {
        Descriptor::Tty { terminal, .. } => Rc::ptr_eq(terminal, &context.terminal),
        Descriptor::Input(input) => Rc::ptr_eq(input, &context.terminal.input),
        Descriptor::Output(output) => Rc::ptr_eq(&output.terminal, &context.terminal),
        _ => false,
    };
    if context.terminal.active.get() && matches {
        Ok(())
    } else {
        Err(-25)
    }
}

pub(crate) fn install(linker: &mut HostLinker) {
    linker
        .func_wrap(
            "efi_process",
            "terminal_get",
            |mut caller: Caller<'_, Context>, fd: u32, ptr: u32| -> i32 {
                let result = (|| {
                    check(&caller, fd)?;
                    let memory = abi::memory(&caller).map_err(|_| -14)?;
                    abi::range(ptr, 12, memory.data_size(&caller)).map_err(|_| -14)?;
                    let terminal = &caller.data().terminal;
                    let (columns, rows) = terminal.size.get();
                    let bytes = [
                        terminal.mode.get().to_le_bytes(),
                        u32::from(columns).to_le_bytes(),
                        u32::from(rows).to_le_bytes(),
                    ]
                    .concat();
                    memory
                        .write(&mut caller, ptr as usize, &bytes)
                        .map_err(|_| -14)
                })();
                result.err().unwrap_or(0)
            },
        )
        .expect("Unique import");
    linker
        .func_wrap(
            "efi_process",
            "terminal_set",
            |caller: Caller<'_, Context>, fd: u32, mode: u32| -> Result<i32, wasmi::Error> {
                if let Err(error) = check(&caller, fd) {
                    return Ok(error);
                }
                if mode & !7 != 0 {
                    return Ok(-22);
                }
                // A scheduling boundary lets the UI observe the mode before input.
                Err(wasmi::Error::host(abi::Action::TerminalMode(mode)))
            },
        )
        .expect("Unique import");
}
