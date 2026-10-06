//! UEFI 2.11 image services and recursive driver connection at application TPL.
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};
use uefi::{
    CString16, Guid, Status, boot,
    fs::FileSystem,
    proto::{
        device_path::{DevicePath, LoadedImageDevicePath, build},
        loaded_image::LoadedImage,
    },
};

pub fn initialize() -> String {
    let mut report = String::new();
    let loading = load_configured(&mut report);
    if let Err(error) = loading {
        report.push_str(&format!("Driver loading stopped: {error}\n"));
    }
    match connect_all() {
        Ok((passes, errors)) => {
            report.push_str(&format!("Driver connection passes: {passes}\n"));
            report.push_str(&errors);
        }
        Err(error) => report.push_str(&format!("Driver connection failed: {error}\n")),
    }
    report.trim_end().into()
}

fn load_configured(report: &mut String) -> Result<(), String> {
    let mut fs = boot::get_image_file_system(boot::image_handle())
        .map(FileSystem::new)
        .map_err(|e| format!("Boot volume: {e}"))?;
    let manifest = uefi::cstr16!("\\EFI\\AGENT\\DRIVERS.JSON");
    if !fs
        .try_exists(manifest)
        .map_err(|e| format!("DRIVERS.JSON: {e:?}"))?
    {
        report.push_str("Extra drivers: no DRIVERS.JSON\n");
        return Ok(());
    }
    if fs
        .metadata(manifest)
        .map_err(|e| format!("DRIVERS.JSON: {e:?}"))?
        .file_size()
        > 65536
    {
        return Err("DRIVERS.JSON exceeds 64 KiB".into());
    }
    let text = fs
        .read_to_string(manifest)
        .map_err(|e| format!("DRIVERS.JSON: {e:?}"))?;
    drop(fs);
    let paths = efi_agent_core::drivers::manifest(&text)?;
    let device = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle())
        .map_err(|e| format!("Agent image: {e}"))?
        .device()
        .ok_or("Agent has no boot device")?;
    let mut total = 0;
    let mut started = 0;
    for path in &paths {
        let mut fs = boot::get_image_file_system(boot::image_handle())
            .map(FileSystem::new)
            .map_err(|e| format!("Boot volume: {e}"))?;
        let file = CString16::try_from(path.as_str()).map_err(|_| "Invalid driver path")?;
        let length = fs
            .metadata(file.as_ref())
            .map_err(|e| format!("{path}: {e:?}"))?
            .file_size();
        if length > efi_agent_core::drivers::MAX_DRIVER_BYTES as u64 {
            return Err(format!("{path}: driver exceeds 8 MiB"));
        }
        total += length;
        if total > efi_agent_core::drivers::MAX_TOTAL_DRIVER_BYTES as u64 {
            return Err("Driver images exceed 32 MiB total".into());
        }
        let bytes = fs
            .read(file.as_ref())
            .map_err(|e| format!("{path}: {e:?}"))?;
        // A driver entry point can open its own image filesystem. Release the
        // exclusive boot-volume protocol before transferring control to it.
        drop(fs);
        efi_agent_core::drivers::validate_image(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let mut storage = Vec::new();
        let mut builder = build::DevicePathBuilder::with_vec(&mut storage);
        {
            let device_path = boot::open_protocol_exclusive::<DevicePath>(device)
                .map_err(|e| format!("Driver volume path: {e}"))?;
            for node in device_path.node_iter() {
                builder = builder
                    .push(&node)
                    .map_err(|e| format!("Driver device path: {e:?}"))?;
            }
        }
        let full_path = builder
            .push(&build::media::FilePath {
                path_name: file.as_ref(),
            })
            .and_then(|b| b.finalize())
            .map_err(|e| format!("Driver file path: {e:?}"))?;
        if image_present(full_path)? {
            report.push_str(&format!("Driver already resident: {path}\n"));
            continue;
        }
        // Pass both the verified bytes and original path. Firmware still applies
        // image validation and its signing policy. No loader bypass is used.
        let image = boot::load_image(
            boot::image_handle(),
            boot::LoadImageSource::FromBuffer {
                buffer: &bytes,
                file_path: Some(full_path),
            },
        )
        .map_err(|e| format!("{path}: LoadImage: {e}"))?;
        boot::start_image(image).map_err(|e| format!("{path}: StartImage: {e}"))?;
        // Successful EFI drivers stay resident after their entry point returns.
        // Unloading would invalidate their installed protocols and callbacks.
        report.push_str(&format!("Driver started: {path}\n"));
        started += 1;
    }
    report.push_str(&format!("Extra drivers started: {started}\n"));
    Ok(())
}

fn image_present(path: &DevicePath) -> Result<bool, String> {
    let handles = match boot::find_handles::<LoadedImageDevicePath>() {
        Ok(handles) => handles,
        Err(error) if error.status() == Status::NOT_FOUND => return Ok(false),
        Err(error) => return Err(format!("Resident image discovery: {error}")),
    };
    for handle in handles {
        // SAFETY: no image is unloaded while this short-lived view is open.
        // GET_PROTOCOL avoids exclusive access to a running firmware driver.
        let loaded = unsafe {
            boot::open_protocol::<LoadedImageDevicePath>(
                boot::OpenProtocolParams {
                    handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                boot::OpenProtocolAttributes::GetProtocol,
            )
        }
        .map_err(|e| format!("Resident image path: {e}"))?;
        if loaded.as_bytes() == path.as_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn snapshot() -> Result<(BTreeSet<usize>, BTreeSet<usize>), String> {
    let handles = boot::locate_handle_buffer(boot::SearchType::AllHandles)
        .map_err(|e| format!("Enumerate controllers: {e}"))?;
    let all = handles
        .iter()
        .map(|handle| handle.as_ptr() as usize)
        .collect();
    let binding = protocol_handles(&uefi_raw::protocol::driver::DriverBindingProtocol::GUID)?;
    Ok((all, binding))
}

fn protocol_handles(guid: &Guid) -> Result<BTreeSet<usize>, String> {
    match boot::locate_handle_buffer(boot::SearchType::ByProtocol(guid)) {
        Ok(handles) => Ok(handles
            .iter()
            .map(|handle| handle.as_ptr() as usize)
            .collect()),
        Err(error) if error.status() == Status::NOT_FOUND => Ok(BTreeSet::new()),
        Err(error) => Err(format!("Protocol discovery: {error}")),
    }
}

fn connect_all() -> Result<(usize, String), String> {
    let mut before = snapshot()?;
    let mut passes = 0;
    let mut failures = BTreeSet::new();
    let mut errors = String::new();
    loop {
        passes += 1;
        let handles = boot::locate_handle_buffer(boot::SearchType::AllHandles)
            .map_err(|e| format!("Enumerate controllers: {e}"))?;
        for &handle in handles.iter() {
            if let Err(error) = boot::connect_controller(handle, &[], None, true)
                && !matches!(
                    error.status(),
                    Status::NOT_FOUND | Status::UNSUPPORTED | Status::ALREADY_STARTED
                )
                && failures.insert((handle.as_ptr() as usize, error.status().0))
            {
                errors.push_str(&format!(
                    "ConnectController {handle:?}: {}\n",
                    error.status()
                ));
            }
        }
        let after = snapshot()?;
        if after == before {
            return Ok((passes, errors));
        }
        before = after;
    }
}

pub fn protocol_count(guid: &Guid) -> String {
    match protocol_handles(guid) {
        Ok(handles) => format!("{}", handles.len()),
        Err(error) => error,
    }
}
