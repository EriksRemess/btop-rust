// Minimal Vulkan C ABI declarations for querying names; no Vulkan SDK is needed.
// Layouts follow Khronos Vulkan-Headers/include/vulkan/vulkan_core.h.
// Copyright 2015-2026 The Khronos Group Inc.
// SPDX-License-Identifier: Apache-2.0 OR MIT
use super::DynamicLibrary;
use std::ffi::{c_char, c_void};
use std::{mem, ptr};

#[repr(C)]
#[allow(non_snake_case)]
struct VkApplicationInfo {
    sType: u32,
    pNext: *const c_void,
    pApplicationName: *const c_char,
    applicationVersion: u32,
    pEngineName: *const c_char,
    engineVersion: u32,
    apiVersion: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkInstanceCreateInfo {
    sType: u32,
    pNext: *const c_void,
    flags: u32,
    pApplicationInfo: *const VkApplicationInfo,
    enabledLayerCount: u32,
    ppEnabledLayerNames: *const *const c_char,
    enabledExtensionCount: u32,
    ppEnabledExtensionNames: *const *const c_char,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkPhysicalDeviceLimits {
    maxImageDimension1D: u32,
    maxImageDimension2D: u32,
    maxImageDimension3D: u32,
    maxImageDimensionCube: u32,
    maxImageArrayLayers: u32,
    maxTexelBufferElements: u32,
    maxUniformBufferRange: u32,
    maxStorageBufferRange: u32,
    maxPushConstantsSize: u32,
    maxMemoryAllocationCount: u32,
    maxSamplerAllocationCount: u32,
    bufferImageGranularity: u64,
    sparseAddressSpaceSize: u64,
    maxBoundDescriptorSets: u32,
    maxPerStageDescriptorSamplers: u32,
    maxPerStageDescriptorUniformBuffers: u32,
    maxPerStageDescriptorStorageBuffers: u32,
    maxPerStageDescriptorSampledImages: u32,
    maxPerStageDescriptorStorageImages: u32,
    maxPerStageDescriptorInputAttachments: u32,
    maxPerStageResources: u32,
    maxDescriptorSetSamplers: u32,
    maxDescriptorSetUniformBuffers: u32,
    maxDescriptorSetUniformBuffersDynamic: u32,
    maxDescriptorSetStorageBuffers: u32,
    maxDescriptorSetStorageBuffersDynamic: u32,
    maxDescriptorSetSampledImages: u32,
    maxDescriptorSetStorageImages: u32,
    maxDescriptorSetInputAttachments: u32,
    maxVertexInputAttributes: u32,
    maxVertexInputBindings: u32,
    maxVertexInputAttributeOffset: u32,
    maxVertexInputBindingStride: u32,
    maxVertexOutputComponents: u32,
    maxTessellationGenerationLevel: u32,
    maxTessellationPatchSize: u32,
    maxTessellationControlPerVertexInputComponents: u32,
    maxTessellationControlPerVertexOutputComponents: u32,
    maxTessellationControlPerPatchOutputComponents: u32,
    maxTessellationControlTotalOutputComponents: u32,
    maxTessellationEvaluationInputComponents: u32,
    maxTessellationEvaluationOutputComponents: u32,
    maxGeometryShaderInvocations: u32,
    maxGeometryInputComponents: u32,
    maxGeometryOutputComponents: u32,
    maxGeometryOutputVertices: u32,
    maxGeometryTotalOutputComponents: u32,
    maxFragmentInputComponents: u32,
    maxFragmentOutputAttachments: u32,
    maxFragmentDualSrcAttachments: u32,
    maxFragmentCombinedOutputResources: u32,
    maxComputeSharedMemorySize: u32,
    maxComputeWorkGroupCount: [u32; 3],
    maxComputeWorkGroupInvocations: u32,
    maxComputeWorkGroupSize: [u32; 3],
    subPixelPrecisionBits: u32,
    subTexelPrecisionBits: u32,
    mipmapPrecisionBits: u32,
    maxDrawIndexedIndexValue: u32,
    maxDrawIndirectCount: u32,
    maxSamplerLodBias: f32,
    maxSamplerAnisotropy: f32,
    maxViewports: u32,
    maxViewportDimensions: [u32; 2],
    viewportBoundsRange: [f32; 2],
    viewportSubPixelBits: u32,
    minMemoryMapAlignment: usize,
    minTexelBufferOffsetAlignment: u64,
    minUniformBufferOffsetAlignment: u64,
    minStorageBufferOffsetAlignment: u64,
    minTexelOffset: i32,
    maxTexelOffset: u32,
    minTexelGatherOffset: i32,
    maxTexelGatherOffset: u32,
    minInterpolationOffset: f32,
    maxInterpolationOffset: f32,
    subPixelInterpolationOffsetBits: u32,
    maxFramebufferWidth: u32,
    maxFramebufferHeight: u32,
    maxFramebufferLayers: u32,
    framebufferColorSampleCounts: u32,
    framebufferDepthSampleCounts: u32,
    framebufferStencilSampleCounts: u32,
    framebufferNoAttachmentsSampleCounts: u32,
    maxColorAttachments: u32,
    sampledImageColorSampleCounts: u32,
    sampledImageIntegerSampleCounts: u32,
    sampledImageDepthSampleCounts: u32,
    sampledImageStencilSampleCounts: u32,
    storageImageSampleCounts: u32,
    maxSampleMaskWords: u32,
    timestampComputeAndGraphics: u32,
    timestampPeriod: f32,
    maxClipDistances: u32,
    maxCullDistances: u32,
    maxCombinedClipAndCullDistances: u32,
    discreteQueuePriorities: u32,
    pointSizeRange: [f32; 2],
    lineWidthRange: [f32; 2],
    pointSizeGranularity: f32,
    lineWidthGranularity: f32,
    strictLines: u32,
    standardSampleLocations: u32,
    optimalBufferCopyOffsetAlignment: u64,
    optimalBufferCopyRowPitchAlignment: u64,
    nonCoherentAtomSize: u64,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkPhysicalDeviceSparseProperties {
    residencyStandard2DBlockShape: u32,
    residencyStandard2DMultisampleBlockShape: u32,
    residencyStandard3DBlockShape: u32,
    residencyAlignedMipSize: u32,
    residencyNonResidentStrict: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkPhysicalDeviceProperties {
    apiVersion: u32,
    driverVersion: u32,
    vendorID: u32,
    deviceID: u32,
    deviceType: u32,
    deviceName: [c_char; 256],
    pipelineCacheUUID: [u8; 16],
    limits: VkPhysicalDeviceLimits,
    sparseProperties: VkPhysicalDeviceSparseProperties,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkPhysicalDeviceProperties2 {
    sType: u32,
    pNext: *mut c_void,
    properties: VkPhysicalDeviceProperties,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkPhysicalDevicePCIBusInfoPropertiesEXT {
    sType: u32,
    pNext: *mut c_void,
    pciDomain: u32,
    pciBus: u32,
    pciDevice: u32,
    pciFunction: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
#[derive(Clone, Copy)]
struct VkExtensionProperties {
    extensionName: [c_char; 256],
    specVersion: u32,
}

impl Default for VkExtensionProperties {
    fn default() -> Self {
        Self {
            extensionName: [0; 256],
            specVersion: 0,
        }
    }
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkConformanceVersion {
    major: u8,
    minor: u8,
    subminor: u8,
    patch: u8,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkPhysicalDeviceDriverProperties {
    sType: u32,
    pNext: *mut c_void,
    driverID: u32,
    driverName: [c_char; 256],
    driverInfo: [c_char; 256],
    conformanceVersion: VkConformanceVersion,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkMemoryHeap {
    size: u64,
    flags: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkMemoryType {
    propertyFlags: u32,
    heapIndex: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct VkPhysicalDeviceMemoryProperties {
    memoryTypeCount: u32,
    memoryTypes: [VkMemoryType; 32],
    memoryHeapCount: u32,
    memoryHeaps: [VkMemoryHeap; 16],
}

type InstanceHandle = *mut c_void;
type PhysicalDevice = *mut c_void;
type CreateInstance =
    unsafe extern "C" fn(*const VkInstanceCreateInfo, *const c_void, *mut InstanceHandle) -> i32;
type DestroyInstance = unsafe extern "C" fn(InstanceHandle, *const c_void);
type EnumerateDevices = unsafe extern "C" fn(InstanceHandle, *mut u32, *mut PhysicalDevice) -> i32;
type EnumerateExtensions = unsafe extern "C" fn(
    PhysicalDevice,
    *const c_char,
    *mut u32,
    *mut VkExtensionProperties,
) -> i32;
type GetMemoryProperties =
    unsafe extern "C" fn(PhysicalDevice, *mut VkPhysicalDeviceMemoryProperties);

type GetProperties = unsafe extern "C" fn(PhysicalDevice, *mut VkPhysicalDeviceProperties2);

struct Instance {
    handle: InstanceHandle,
    destroy: DestroyInstance,
    _library: DynamicLibrary,
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.handle, ptr::null()) };
    }
}

#[derive(Debug, Default)]
pub(super) struct DeviceInfo {
    vendor: u32,
    device: u32,
    pci: Option<[u32; 4]>,
    pub(super) name: String,
    pub(super) api_version: u32,
    pub(super) device_type: u32,
    pub(super) driver_name: Option<String>,
    driver_info: Option<String>,
    driver_version: u32,
    heaps: Vec<MemoryHeap>,
}

#[derive(Debug)]
struct MemoryHeap {
    size: u64,
    device_local: bool,
    host_visible: bool,
}

fn string(buffer: &[c_char]) -> Option<String> {
    let end = buffer.iter().position(|&ch| ch == 0)?;
    let bytes: Vec<_> = buffer[..end].iter().map(|&ch| ch as u8).collect();
    String::from_utf8(bytes)
        .ok()
        .filter(|name| !name.trim().is_empty())
}

// Vulkan permits VK_INCOMPLETE when the count changes between calls. Retry
// with a fresh count, and bound both attempts and allocations.
fn enumerate<T: Copy + Default>(mut call: impl FnMut(*mut u32, *mut T) -> i32) -> Option<Vec<T>> {
    for _ in 0..3 {
        let mut count = 0;
        if call(&mut count, ptr::null_mut()) != 0 || count > 4096 {
            return None;
        }
        let mut values = vec![T::default(); count as usize];
        let capacity = count;
        let status = call(&mut count, values.as_mut_ptr());
        if count > capacity {
            return None;
        }
        if status == 0 {
            values.truncate(count as usize);
            return Some(values);
        }
        if status != 5 {
            // VK_INCOMPLETE
            return None;
        }
    }
    None
}

pub(super) fn devices() -> Result<Vec<DeviceInfo>, String> {
    query_devices(false)
}

fn query_devices(include_software: bool) -> Result<Vec<DeviceInfo>, String> {
    let library = DynamicLibrary::open(&["libvulkan.so.1", "libvulkan.so"])
        .ok_or("Vulkan loader library is unavailable")?;
    let create: CreateInstance = library
        .symbol(b"vkCreateInstance\0")
        .ok_or("Vulkan loader is missing vkCreateInstance")?;
    let destroy: DestroyInstance = library
        .symbol(b"vkDestroyInstance\0")
        .ok_or("Vulkan loader is missing vkDestroyInstance")?;
    let devices: EnumerateDevices = library
        .symbol(b"vkEnumeratePhysicalDevices\0")
        .ok_or("Vulkan loader is missing vkEnumeratePhysicalDevices")?;
    let extensions: EnumerateExtensions = library
        .symbol(b"vkEnumerateDeviceExtensionProperties\0")
        .ok_or("Vulkan loader is missing vkEnumerateDeviceExtensionProperties")?;
    let properties: GetProperties = library
        .symbol(b"vkGetPhysicalDeviceProperties2\0")
        .ok_or("Vulkan loader is missing vkGetPhysicalDeviceProperties2")?;
    let memory_properties: Option<GetMemoryProperties> =
        library.symbol(b"vkGetPhysicalDeviceMemoryProperties\0");
    let app = VkApplicationInfo {
        sType: 0,
        pNext: ptr::null(),
        pApplicationName: c"btoprs".as_ptr(),
        applicationVersion: 0,
        pEngineName: ptr::null(),
        engineVersion: 0,
        apiVersion: (1 << 22) | (1 << 12), // Vulkan 1.1, for properties2
    };
    let info = VkInstanceCreateInfo {
        sType: 1,
        pNext: ptr::null(),
        flags: 0,
        pApplicationInfo: &app,
        enabledLayerCount: 0,
        ppEnabledLayerNames: ptr::null(),
        enabledExtensionCount: 0,
        ppEnabledExtensionNames: ptr::null(),
    };
    let mut handle = ptr::null_mut();
    let status = unsafe { create(&info, ptr::null(), &mut handle) };
    if status != 0 || handle.is_null() {
        return Err(format!(
            "vkCreateInstance failed (VkResult {status}); check the installed Vulkan ICD and driver access"
        ));
    }
    let instance = Instance {
        handle,
        destroy,
        _library: library,
    };
    let devices = enumerate(|count, output| unsafe { devices(instance.handle, count, output) })
        .ok_or("Vulkan physical-device enumeration failed")?;
    let mut names = Vec::new();
    for device in devices {
        let extensions =
            enumerate(|count, output| unsafe { extensions(device, ptr::null(), count, output) })
                .unwrap_or_default();
        let supports = |name| {
            extensions
                .iter()
                .any(|ext| string(&ext.extensionName).as_deref() == Some(name))
        };
        let supports_pci = supports("VK_EXT_pci_bus_info");
        let mut pci = VkPhysicalDevicePCIBusInfoPropertiesEXT {
            sType: 1000212000,
            pNext: ptr::null_mut(),
            pciDomain: 0,
            pciBus: 0,
            pciDevice: 0,
            pciFunction: 0,
        };
        // These C structures contain only numeric fields, arrays and pointers.
        let mut value: VkPhysicalDeviceProperties2 = unsafe { mem::zeroed() };
        value.sType = 1000059001;
        if supports_pci {
            value.pNext = (&mut pci as *mut VkPhysicalDevicePCIBusInfoPropertiesEXT).cast();
        }
        unsafe { properties(device, &mut value) };
        // CPU/software Vulkan implementations are not physical GPU names.
        if value.properties.deviceType == 4 && !include_software {
            continue;
        }
        let mut driver: VkPhysicalDeviceDriverProperties = unsafe { mem::zeroed() };
        if value.properties.apiVersion >= ((1 << 22) | (2 << 12))
            || supports("VK_KHR_driver_properties")
        {
            driver.sType = 1000196000;
            driver.pNext = value.pNext;
            value.pNext = (&mut driver as *mut VkPhysicalDeviceDriverProperties).cast();
            unsafe { properties(device, &mut value) };
        }
        let heaps = memory_properties
            .map(|query| {
                let mut memory: VkPhysicalDeviceMemoryProperties = unsafe { mem::zeroed() };
                unsafe { query(device, &mut memory) };
                memory_heaps(&memory)
            })
            .unwrap_or_default();
        if let Some(name) = string(&value.properties.deviceName) {
            names.push(DeviceInfo {
                vendor: value.properties.vendorID,
                device: value.properties.deviceID,
                pci: supports_pci.then_some([
                    pci.pciDomain,
                    pci.pciBus,
                    pci.pciDevice,
                    pci.pciFunction,
                ]),
                name,
                api_version: value.properties.apiVersion,
                device_type: value.properties.deviceType,
                driver_name: string(&driver.driverName),
                driver_info: string(&driver.driverInfo),
                driver_version: value.properties.driverVersion,
                heaps,
            });
        }
    }
    Ok(names)
}

pub(super) fn info_for_device<'a>(
    names: &'a [DeviceInfo],
    vendor: u32,
    device: u32,
    address: &str,
) -> Option<&'a DeviceInfo> {
    let parts: Vec<_> = address
        .split([':', '.'])
        .map(|part| u32::from_str_radix(part, 16))
        .collect::<Result<_, _>>()
        .ok()?;
    let pci: [u32; 4] = parts.try_into().ok()?;
    let candidates: Vec<_> = names
        .iter()
        .filter(|name| name.vendor == vendor && name.device == device)
        .collect();
    if let Some(name) = candidates.iter().find(|name| name.pci == Some(pci)) {
        return Some(name);
    }
    // A unique vendor/device pair is usable if the driver lacks PCI address
    // support. Never use a name whose reported address disagrees with sysfs.
    if candidates.len() == 1 && candidates[0].pci.is_none() {
        Some(candidates[0])
    } else {
        None
    }
}

pub(super) fn name_for_device<'a>(
    names: &'a [DeviceInfo],
    vendor: u32,
    device: u32,
    address: &str,
) -> Option<&'a str> {
    info_for_device(names, vendor, device, address).map(|info| info.name.as_str())
}

fn memory_heaps(memory: &VkPhysicalDeviceMemoryProperties) -> Vec<MemoryHeap> {
    if memory.memoryHeapCount as usize > memory.memoryHeaps.len()
        || memory.memoryTypeCount as usize > memory.memoryTypes.len()
    {
        return Vec::new();
    }
    memory.memoryHeaps[..memory.memoryHeapCount as usize]
        .iter()
        .enumerate()
        .map(|(index, heap)| MemoryHeap {
            size: heap.size,
            device_local: heap.flags & 1 != 0,
            host_visible: memory.memoryTypes[..memory.memoryTypeCount as usize]
                .iter()
                .any(|kind| kind.heapIndex as usize == index && kind.propertyFlags & 2 != 0),
        })
        .collect()
}

pub(super) fn api_version(version: u32) -> String {
    format!(
        "{}.{}.{}",
        (version >> 22) & 0x7f,
        (version >> 12) & 0x3ff,
        version & 0xfff
    )
}

pub(super) fn diagnostics(devices: &[DeviceInfo]) -> String {
    let mut output = String::from("\nVulkan device information\n");
    if devices.is_empty() {
        output.push_str("  No accessible physical GPUs\n");
    }
    for device in devices {
        output.push_str(&format!(
            "  device: {}\n  type: {}\n  Vulkan API: {}\n",
            device.name,
            match device.device_type {
                1 => "integrated",
                2 => "discrete",
                3 => "virtual",
                _ => "other",
            },
            api_version(device.api_version)
        ));
        if let Some(name) = &device.driver_name {
            output.push_str(&format!("  driver: {name}\n"));
        }
        if let Some(info) = &device.driver_info {
            output.push_str(&format!("  driver details: {info}\n"));
        }
        // The raw driver version has a vendor-specific encoding.
        output.push_str(&format!(
            "  driver version (raw): {:#x}\n",
            device.driver_version
        ));
        for (index, heap) in device.heaps.iter().enumerate() {
            output.push_str(&format!(
                "  memory heap {index}: {}{}{}\n",
                crate::units::bytes(heap.size, false),
                if heap.device_local {
                    ", device-local"
                } else {
                    ""
                },
                if heap.host_visible {
                    ", host-visible"
                } else {
                    ""
                }
            ));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_pci_address_and_reject_ambiguous_devices() {
        let make = |pci, name: &str| DeviceInfo {
            vendor: 0x1002,
            device: 0x1638,
            pci,
            name: name.into(),
            ..DeviceInfo::default()
        };
        let mut names = vec![
            make(Some([0, 4, 0, 0]), "Driver name A"),
            make(Some([0, 5, 0, 0]), "Driver name B"),
        ];
        assert_eq!(
            name_for_device(&names, 0x1002, 0x1638, "0000:05:00.0"),
            Some("Driver name B")
        );
        assert_eq!(
            name_for_device(&names, 0x1002, 0x1638, "0000:06:00.0"),
            None
        );
        assert_eq!(
            name_for_device(&names, 0x8086, 0x1638, "0000:05:00.0"),
            None
        );
        names = vec![make(None, "Driver name A")];
        assert_eq!(
            name_for_device(&names, 0x1002, 0x1638, "0000:05:00.0"),
            Some("Driver name A")
        );
        names.push(make(None, "Driver name B"));
        assert_eq!(
            name_for_device(&names, 0x1002, 0x1638, "0000:05:00.0"),
            None
        );
    }

    #[test]
    fn enumeration_retries_incomplete_results_and_handles_errors() {
        let mut calls = 0;
        let values = enumerate::<u32>(|count, output| {
            calls += 1;
            unsafe {
                if output.is_null() {
                    *count = 1;
                    return 0;
                }
                *output = 42;
            }
            if calls == 2 { 5 } else { 0 }
        });
        assert_eq!(values, Some(vec![42]));
        assert_eq!(calls, 4);
        assert_eq!(enumerate::<u32>(|_, _| -1), None);
    }

    #[test]
    fn device_names_are_bounded_utf8() {
        assert_eq!(string(&[65, 0]), Some("A".into()));
        assert_eq!(string(&[65, 66]), None);
        assert_eq!(string(&[0]), None);
    }

    #[test]
    fn memory_types_sharing_a_heap_do_not_duplicate_capacity() {
        let mut memory: VkPhysicalDeviceMemoryProperties = unsafe { mem::zeroed() };
        memory.memoryHeapCount = 1;
        memory.memoryHeaps[0] = VkMemoryHeap {
            size: 512 * 1024 * 1024,
            flags: 1,
        };
        memory.memoryTypeCount = 2;
        memory.memoryTypes[0] = VkMemoryType {
            propertyFlags: 1,
            heapIndex: 0,
        };
        memory.memoryTypes[1] = VkMemoryType {
            propertyFlags: 3,
            heapIndex: 0,
        };
        let heaps = memory_heaps(&memory);
        assert_eq!(heaps.len(), 1);
        assert_eq!(heaps[0].size, 512 * 1024 * 1024);
        assert!(heaps[0].device_local && heaps[0].host_visible);
        memory.memoryHeapCount = 17;
        assert!(memory_heaps(&memory).is_empty());
    }

    #[test]
    fn diagnostics_distinguish_heap_capacity_from_live_usage() {
        let device = DeviceInfo {
            name: "Driver supplied GPU".into(),
            device_type: 1,
            api_version: (1 << 22) | (3 << 12) | 7,
            driver_name: Some("Example driver".into()),
            driver_info: Some("Build 42".into()),
            heaps: vec![MemoryHeap {
                size: 512 * 1024 * 1024,
                device_local: true,
                host_visible: true,
            }],
            ..DeviceInfo::default()
        };
        let output = diagnostics(&[device]);
        assert!(output.contains("type: integrated"));
        assert!(output.contains("Vulkan API: 1.3.7"));
        assert!(output.contains("driver: Example driver"));
        assert!(output.contains("driver details: Build 42"));
        assert!(output.contains("memory heap 0: 512 MiB, device-local, host-visible"));
        assert!(!output.contains("usage"));
        assert!(!output.contains("VRAM"));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn vulkan_abi_matches_khronos_headers() {
        assert_eq!(mem::size_of::<VkPhysicalDeviceDriverProperties>(), 536);
        assert_eq!(mem::size_of::<VkPhysicalDeviceMemoryProperties>(), 520);
        assert_eq!(mem::size_of::<VkMemoryHeap>(), 16);
        assert_eq!(mem::size_of::<VkApplicationInfo>(), 48);
        assert_eq!(mem::size_of::<VkInstanceCreateInfo>(), 64);
        assert_eq!(mem::size_of::<VkPhysicalDeviceLimits>(), 504);
        assert_eq!(mem::size_of::<VkPhysicalDeviceProperties>(), 824);
        assert_eq!(mem::size_of::<VkPhysicalDeviceProperties2>(), 840);
        assert_eq!(
            mem::size_of::<VkPhysicalDevicePCIBusInfoPropertiesEXT>(),
            32
        );
        assert_eq!(mem::offset_of!(VkPhysicalDeviceProperties, deviceName), 20);
        assert_eq!(mem::offset_of!(VkPhysicalDeviceProperties, limits), 296);
    }

    #[test]
    #[ignore = "requires a Vulkan loader and driver on the host"]
    fn queries_installed_vulkan_driver() {
        let names = query_devices(true).expect("Vulkan driver query failed");
        assert!(!names.is_empty());
        assert!(
            names
                .iter()
                .all(|device| device.api_version > 0 && !device.heaps.is_empty())
        );
        eprintln!("Vulkan physical GPU names: {names:?}");
        assert!(names.iter().all(|device| !device.name.is_empty()));
    }
}
