//! Integration fixture: a resident driver that advertises a binding and writes
//! a marker on its actual boot volume. It does not control any physical device.
#![no_std]
#![no_main]
extern crate alloc;

use alloc::boxed::Box;
use uefi::{Status, boot, fs::FileSystem, prelude::*};
use uefi_raw::{
    Handle,
    protocol::{device_path::DevicePathProtocol, driver::DriverBindingProtocol},
};

unsafe extern "efiapi" fn supported(
    _: *const DriverBindingProtocol,
    _: Handle,
    _: *const DevicePathProtocol,
) -> Status {
    Status::UNSUPPORTED
}
unsafe extern "efiapi" fn start(
    _: *const DriverBindingProtocol,
    _: Handle,
    _: *const DevicePathProtocol,
) -> Status {
    panic!("Unsupported probe driver must not be connected")
}
unsafe extern "efiapi" fn stop(
    _: *const DriverBindingProtocol,
    _: Handle,
    _: usize,
    _: *const Handle,
) -> Status {
    panic!("Unsupported probe driver must not be disconnected")
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    let handle = boot::image_handle();
    let mut fs = FileSystem::new(boot::get_image_file_system(handle).unwrap());
    let marker = uefi::cstr16!("\\work\\driver-started.txt");
    // Count entry point executions so the test can detect unintended reloads.
    let mut count = if fs.try_exists(marker).unwrap() {
        fs.read(marker).unwrap()
    } else {
        alloc::vec::Vec::new()
    };
    count.push(b'1');
    fs.write(marker, &count).unwrap();
    let binding = Box::leak(Box::new(DriverBindingProtocol {
        supported,
        start,
        stop,
        version: 0x10,
        image_handle: handle.as_ptr(),
        driver_binding_handle: handle.as_ptr(),
    }));
    // SAFETY: the binding and its function pointers remain in the resident
    // boot-service driver. They are not stack allocated or freed on return.
    unsafe {
        boot::install_protocol_interface(
            Some(handle),
            &DriverBindingProtocol::GUID,
            (binding as *mut DriverBindingProtocol).cast(),
        )
    }
    .unwrap();
    Status::SUCCESS
}
