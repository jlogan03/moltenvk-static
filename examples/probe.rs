#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "system" fn get_instance_proc_addr(
    instance: ash::vk::Instance,
    name: *const core::ffi::c_char,
) -> ash::vk::PFN_vkVoidFunction {
    use ash::vk::Handle;

    // Vulkan represents dispatchable handles as opaque pointers. This target
    // has 64-bit pointers, so ash's raw handle preserves the pointer value.
    unsafe { moltenvk_static::vkGetInstanceProcAddr(instance.as_raw() as *mut _, name) }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The entry borrows functions from a library linked for the process lifetime.
    let entry = unsafe {
        ash::Entry::from_static_fn(ash::StaticFn {
            get_instance_proc_addr,
        })
    };
    let info = ash::vk::ApplicationInfo::default().api_version(ash::vk::API_VERSION_1_2);
    let create = ash::vk::InstanceCreateInfo::default().application_info(&info);
    let instance = unsafe { entry.create_instance(&create, None)? };
    let devices = unsafe { instance.enumerate_physical_devices() };
    let result = devices.map(|devices| {
        for device in &devices {
            let properties = unsafe { instance.get_physical_device_properties(*device) };
            let name = unsafe { core::ffi::CStr::from_ptr(properties.device_name.as_ptr()) };
            println!("{}", name.to_string_lossy());
        }
        devices.len()
    });
    unsafe { instance.destroy_instance(None) };
    assert!(result? > 0, "MoltenVK found no GPU");
    Ok(())
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("This probe requires ARM64 macOS.");
    std::process::exit(1);
}
