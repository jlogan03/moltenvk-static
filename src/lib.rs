//! Static MoltenVK for ARM64 macOS 14 and later.
//!
//! Pass [`vkGetInstanceProcAddr`] to a Vulkan loader adapter. This crate does
//! not initialize Vulkan or change kernel compilation. Its binding uses the
//! Vulkan C ABI and does not depend on a Rust Vulkan binding's types.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "system" {
    /// Returns a Vulkan function pointer from the linked MoltenVK implementation.
    ///
    /// # Safety
    /// `name` must point to a null-terminated name. `instance` is Vulkan's opaque
    /// dispatchable instance pointer, or null where the requested function permits
    /// it. Both arguments must satisfy Vulkan's `vkGetInstanceProcAddr` requirements.
    pub fn vkGetInstanceProcAddr(
        instance: *mut core::ffi::c_void,
        name: *const core::ffi::c_char,
    ) -> Option<unsafe extern "system" fn()>;
}
