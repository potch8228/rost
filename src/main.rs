#![no_main]
#![no_std]
#![feature(offset_of)]

use core::panic::PanicInfo;
use core::time::Duration;
use rost::error;
use rost::executor::sleep;
use rost::executor::spawn_global;
use rost::executor::start_global_executor;
use rost::hpet::global_timestamp;
use rost::info;
use rost::init::init_allocator;
use rost::init::init_basic_runtime;
use rost::init::init_display;
use rost::init::init_hpet;
use rost::init::init_paging;
use rost::init::init_pci;
use rost::print::hexdump_struct;
use rost::print::set_global_vram;
use rost::println;
use rost::qemu::exit_qemu;
use rost::qemu::QemuExitCode;
use rost::serial::SerialPort;
use rost::uefi::init_vram;
use rost::uefi::locate_loaded_image_protocol;
use rost::uefi::EfiHandle;
use rost::uefi::EfiSystemTable;
use rost::warn;
use rost::x86::init_exceptions;

#[no_mangle]
fn efi_main(image_handle: EfiHandle, efi_system_table: &EfiSystemTable) {
    println!("Booting ROSt...\n");
    println!("image_handle: {:#018X}\n", image_handle);
    println!("efi_system_table: {:#p}\n", efi_system_table);
    let loaded_image_protocol = locate_loaded_image_protocol(image_handle, efi_system_table)
        .expect("Failed to get LoadedImageProtocol");
    println!("image_base: {:#018X}", loaded_image_protocol.image_base);
    println!("image_size: {:#018X}", loaded_image_protocol.image_size);
    info!("info");
    warn!("warn");
    error!("error");
    hexdump_struct(efi_system_table);

    let mut vram = init_vram(efi_system_table).expect("init_vram failed");

    init_display(&mut vram);
    set_global_vram(vram);

    let acpi = efi_system_table.acpi_table().expect("ACPI table not found");

    let memory_map = init_basic_runtime(image_handle, efi_system_table);

    init_allocator(&memory_map);

    let (_gdt, _idt) = init_exceptions();

    init_paging(&memory_map);

    init_hpet(acpi);

    init_pci(acpi);

    let t0 = global_timestamp();
    let task1 = async move {
        for i in 100..=103 {
            info!("{i} hpet.main_counter = {:?}", global_timestamp() - t0);
            sleep(Duration::from_secs(1)).await;
        }
        Ok(())
    };
    let task2 = async move {
        for i in 200..=203 {
            info!("{i} hpet.main_counter = {:?}", global_timestamp() - t0);
            sleep(Duration::from_secs(1)).await;
        }
        Ok(())
    };
    let serial_task = async {
        let sp = SerialPort::default();
        if let Err(e) = sp.loopback_test() {
            error!("{e:?}");
            return Err("serial: loopback test failed");
        }
        info!("Started to monitor serial port");
        loop {
            if let Some(v) = sp.try_read() {
                let c = char::from_u32(v as u32);
                info!("serial input: {v:#04X} = {c:?}");
            }
            sleep(Duration::from_millis(20)).await;
        }
    };
    spawn_global(task1);
    spawn_global(task2);
    spawn_global(serial_task);
    start_global_executor()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("PANIC: {info:?}");
    exit_qemu(QemuExitCode::Fail);
}
