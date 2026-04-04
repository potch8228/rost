#![no_main]
#![no_std]
#![feature(offset_of)]

use core::fmt::Write;
use core::panic::PanicInfo;
use core::time::Duration;
use core::writeln;
use rost::error;
// use rost::executor::yield_execution;
use rost::executor::Executor;
use rost::executor::Task;
use rost::executor::TimeoutFuture;
use rost::graphics::draw_test_pattern;
use rost::graphics::fill_rect;
use rost::graphics::Bitmap;
use rost::hpet::global_timestamp;
use rost::hpet::set_global_hpet;
use rost::hpet::Hpet;
use rost::info;
use rost::init::init_basic_runtime;
use rost::init::init_paging;
use rost::print::hexdump;
use rost::println;
use rost::qemu::exit_qemu;
use rost::qemu::QemuExitCode;
use rost::uefi::init_vram;
use rost::uefi::locate_loaded_image_protocol;
use rost::uefi::EfiHandle;
use rost::uefi::EfiMemoryType;
use rost::uefi::EfiSystemTable;
use rost::uefi::VramTextWriter;
use rost::warn;
use rost::x86::flush_tlb;
use rost::x86::init_exceptions;
use rost::x86::read_cr3;
use rost::x86::trigger_debug_interrupt;
use rost::x86::PageAttr;

// static mut GLOBAL_HPET: Option<Hpet> = None;

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
    hexdump(efi_system_table);
    let mut vram = init_vram(efi_system_table).expect("init_vram failed");
    let vw = vram.width();
    let vh = vram.height();
    fill_rect(&mut vram, 0x000000, 0, 0, vw, vh).expect("fill_rect failed");

    draw_test_pattern(&mut vram);

    let mut w = VramTextWriter::new(&mut vram);

    let acpi = efi_system_table.acpi_table().expect("ACPI table not found");

    let memory_map = init_basic_runtime(image_handle, efi_system_table);
    let mut total_memory_pages = 0;
    for e in memory_map.iter() {
        if e.memory_type() != EfiMemoryType::CONVENTIONAL_MEMORY {
            continue;
        }
        total_memory_pages += e.number_of_pages();

        writeln!(w, "{e:?}").unwrap();
    }
    let total_memory_size_mib = total_memory_pages * 4096 / 1024 / 1024;
    writeln!(
        w,
        "Total: {total_memory_pages} pages = {total_memory_size_mib} MiB"
    )
    .unwrap();

    writeln!(w, "Hello, Non-UEFI world!").unwrap();
    let cr3 = rost::x86::read_cr3();
    println!("cr3 = {cr3:#p}");
    let t = Some(unsafe { &*cr3 });
    println!("{t:?}");
    let t = t.and_then(|t| t.next_level(0));
    println!("{t:?}");
    let t = t.and_then(|t| t.next_level(0));
    println!("{t:?}");
    let t = t.and_then(|t| t.next_level(0));
    println!("{t:?}");

    let (_gdt, _idt) = init_exceptions();
    info!("Exception initialized!");
    trigger_debug_interrupt();
    info!("Exception continued");
    init_paging(&memory_map);
    info!("Now we are using our own page tables!");

    let page_table = read_cr3();
    unsafe {
        (*page_table)
            .create_mapping(0, 4096, 0, PageAttr::NotPresent)
            .expect("Failed to unmap page 0");
    }
    flush_tlb();

    let hpet = acpi.hpet().expect("Failed to get HPET from ACPI");
    let hpet = hpet
        .base_address()
        .expect("Failed to get HPET base address");
    info!("HPET is at {hpet:#p}");

    let hpet = Hpet::new(hpet);
    // let hpet = unsafe { GLOBAL_HPET.insert(hpet) };
    // let task1 = Task::new(async {
    set_global_hpet(hpet);
    let t0 = global_timestamp();
    let task1 = Task::new(async move {
        for i in 100..=103 {
            // info!("{i} hpet.main_counter = {}", hpet.main_counter());
            info!("{i} hpet.main_counter = {:?}", global_timestamp() - t0);
            // yield_execution().await;
            TimeoutFuture::new(Duration::from_secs(1)).await;
        }
        Ok(())
    });
    let task2 = Task::new(async move {
        for i in 200..=203 {
            // info!("{i} hpet.main_counter = {}", hpet.main_counter());
            info!("{i} hpet.main_counter = {:?}", global_timestamp() - t0);
            // yield_execution().await;
            TimeoutFuture::new(Duration::from_secs(1)).await;
        }
        Ok(())
    });
    let mut executor = Executor::new();
    executor.enqueue(task1);
    executor.enqueue(task2);
    Executor::run(executor);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("PANIC: {info:?}");
    exit_qemu(QemuExitCode::Fail);
}
