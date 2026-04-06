//! Vulkan rendering with `ash`: swapchain, depth, instanced mesh pipeline.

use ash::khr::{surface, swapchain};
use ash::vk;
use ash::vk::Handle;
use ash::{Device, Entry, Instance};
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::CStr;
use std::mem::size_of;
use std::slice;
use thiserror::Error;
use tracing::{error, info, warn};
use winit::window::Window;

const MAX_FRAMES_IN_FLIGHT: usize = 2;
/// Max instanced draws per frame (uniform buffer path).
pub const MAX_INSTANCES: usize = 16_384;

static MESH_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mesh.spv"));

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("vulkan: {0:?}")]
    Vulkan(vk::Result),
    #[error("{0}")]
    Msg(String),
}

impl From<vk::Result> for RenderError {
    fn from(e: vk::Result) -> Self {
        RenderError::Vulkan(e)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalsUbo {
    view_proj: [[f32; 4]; 4],
}

fn cube_vertices() -> (Vec<Vertex>, Vec<u32>) {
    let c = [
        ([0.0, 0.0, 0.0], [0.2, 0.8, 0.3, 1.0]),
        ([1.0, 0.0, 0.0], [0.3, 0.2, 0.9, 1.0]),
        ([1.0, 1.0, 0.0], [0.9, 0.7, 0.1, 1.0]),
        ([0.0, 1.0, 0.0], [0.1, 0.5, 0.9, 1.0]),
        ([0.0, 0.0, 1.0], [0.8, 0.2, 0.2, 1.0]),
        ([1.0, 0.0, 1.0], [0.5, 0.9, 0.5, 1.0]),
        ([1.0, 1.0, 1.0], [0.9, 0.5, 0.9, 1.0]),
        ([0.0, 1.0, 1.0], [0.4, 0.4, 0.9, 1.0]),
    ];
    let verts: Vec<Vertex> = c
        .iter()
        .map(|(p, col)| Vertex {
            pos: *p,
            color: *col,
        })
        .collect();
    let idx: Vec<u32> = vec![
        0, 2, 1, 2, 0, 3, // bottom (match outward winding for back-face culling)
        4, 5, 6, 6, 7, 4, // top
        0, 1, 5, 5, 4, 0, 1, 2, 6, 6, 5, 1, 2, 3, 7, 7, 6, 2, 3, 0, 4, 4, 7, 3,
    ];
    (verts, idx)
}

fn debug_disable_backface_cull() -> bool {
    std::env::var_os("VGE_DISABLE_BACKFACE_CULL").is_some()
}

/// `SurfaceCapabilitiesKHR::current_extent` is `(0,0)` on some Win32 surfaces (e.g. child windows)
/// until layout; treating that as valid makes `vkCreateSwapchainKHR` validation fail.
///
/// `inner_w` / `inner_h` are the surface size to target (from the window or an explicit override when
/// `request_inner_size` has not been applied yet).
///
/// When `force_from_inner` is true (embedded resize with known egui pixels), ignore
/// `caps.current_extent` so we do not keep a stale extent before the OS updates the HWND.
fn pick_swapchain_extent(
    caps: &vk::SurfaceCapabilitiesKHR,
    inner_w: u32,
    inner_h: u32,
    force_from_inner: bool,
) -> vk::Extent2D {
    let clamp_wh = |w: u32, h: u32| vk::Extent2D {
        width: w.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
        height: h.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
    };

    let mut extent = if !force_from_inner
        && caps.current_extent.width != u32::MAX
        && caps.current_extent.height != u32::MAX
    {
        caps.current_extent
    } else {
        clamp_wh(inner_w, inner_h)
    };

    if extent.width == 0 || extent.height == 0 {
        extent = clamp_wh(inner_w.max(1), inner_h.max(1));
    }
    if extent.width == 0 || extent.height == 0 {
        extent = vk::Extent2D {
            width: caps.min_image_extent.width.max(1),
            height: caps.min_image_extent.height.max(1),
        };
    }
    extent
}

pub struct VulkanRenderer {
    _entry: Entry,
    instance: Instance,
    surface_loader: surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    device: Device,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    graphics_family: u32,
    present_family: u32,
    swapchain_loader: swapchain::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    swapchain_images: Vec<vk::Image>,
    swapchain_views: Vec<vk::ImageView>,
    render_pass: vk::RenderPass,
    depth_image: vk::Image,
    depth_mem: vk::DeviceMemory,
    depth_view: vk::ImageView,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    framebuffers: Vec<vk::Framebuffer>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    /// One per swapchain image — avoids reusing a binary semaphore while present still holds it.
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    current_frame: usize,
    vertex_buffer: vk::Buffer,
    vertex_mem: vk::DeviceMemory,
    index_buffer: vk::Buffer,
    index_mem: vk::DeviceMemory,
    instance_buffer: vk::Buffer,
    instance_mem: vk::DeviceMemory,
    uniform_buffers: Vec<vk::Buffer>,
    uniform_memories: Vec<vk::DeviceMemory>,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    descriptor_layout: vk::DescriptorSetLayout,
    index_count: u32,
    #[cfg(debug_assertions)]
    _debug_loader: ash::ext::debug_utils::Instance,
    #[cfg(debug_assertions)]
    _debug_messenger: vk::DebugUtilsMessengerEXT,
    vsync_enabled: bool,
}

impl VulkanRenderer {
    /// # Safety
    /// `window` must provide valid raw display and window handles for surface creation on the target platform.
    pub unsafe fn new(window: &Window) -> Result<Self, RenderError> {
        let entry = Entry::load().map_err(|e| RenderError::Msg(e.to_string()))?;
        let app_name = c"VoxelEngine";

        let raw_display = window
            .display_handle()
            .map_err(|e| RenderError::Msg(e.to_string()))?
            .as_raw();
        let raw_window = window
            .window_handle()
            .map_err(|e| RenderError::Msg(e.to_string()))?
            .as_raw();

        let surface_extensions =
            ash_window::enumerate_required_extensions(raw_display).map_err(RenderError::Vulkan)?;

        let mut extension_names_raw: Vec<*const i8> = surface_extensions.to_vec();

        #[cfg(debug_assertions)]
        extension_names_raw.push(ash::ext::debug_utils::NAME.as_ptr());

        let layer_names = if cfg!(debug_assertions) {
            vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
        } else {
            vec![]
        };

        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .api_version(vk::make_api_version(0, 1, 3, 0));

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names_raw)
            .enabled_layer_names(&layer_names);

        let instance = entry.create_instance(&create_info, None)?;

        #[cfg(debug_assertions)]
        let (debug_loader, debug_messenger) = {
            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                )
                .pfn_user_callback(Some(debug_callback));
            let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let messenger = loader.create_debug_utils_messenger(&debug_info, None)?;
            (loader, messenger)
        };

        let surface_loader = surface::Instance::new(&entry, &instance);
        let surface = ash_window::create_surface(&entry, &instance, raw_display, raw_window, None)?;

        let physical_device = pick_physical_device(&instance, &surface_loader, surface)?;
        let (graphics_family, present_family) =
            find_queue_families(&instance, physical_device, &surface_loader, surface)?;

        let device_extensions = [swapchain::NAME.as_ptr()];
        let priorities = [1.0f32];
        let queue_create_infos = if graphics_family == present_family {
            vec![vk::DeviceQueueCreateInfo::default()
                .queue_family_index(graphics_family)
                .queue_priorities(&priorities)]
        } else {
            vec![
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(graphics_family)
                    .queue_priorities(&priorities),
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(present_family)
                    .queue_priorities(&priorities),
            ]
        };

        let device_features = vk::PhysicalDeviceFeatures::default();

        let device_create = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&device_extensions)
            .enabled_features(&device_features);

        let device = instance.create_device(physical_device, &device_create, None)?;
        let graphics_queue = device.get_device_queue(graphics_family, 0);
        let present_queue = device.get_device_queue(present_family, 0);

        let swapchain_loader = swapchain::Device::new(&instance, &device);
        let mut r = Self {
            _entry: entry,
            instance,
            surface_loader,
            surface,
            physical_device,
            device,
            graphics_queue,
            present_queue,
            graphics_family,
            present_family,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            swapchain_format: vk::Format::UNDEFINED,
            swapchain_extent: vk::Extent2D::default(),
            swapchain_images: vec![],
            swapchain_views: vec![],
            render_pass: vk::RenderPass::null(),
            depth_image: vk::Image::null(),
            depth_mem: vk::DeviceMemory::null(),
            depth_view: vk::ImageView::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            framebuffers: vec![],
            command_pool: vk::CommandPool::null(),
            command_buffers: vec![],
            image_available: vec![],
            render_finished: vec![],
            in_flight: vec![],
            current_frame: 0,
            vertex_buffer: vk::Buffer::null(),
            vertex_mem: vk::DeviceMemory::null(),
            index_buffer: vk::Buffer::null(),
            index_mem: vk::DeviceMemory::null(),
            instance_buffer: vk::Buffer::null(),
            instance_mem: vk::DeviceMemory::null(),
            uniform_buffers: vec![],
            uniform_memories: vec![],
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: vec![],
            descriptor_layout: vk::DescriptorSetLayout::null(),
            index_count: 0,
            #[cfg(debug_assertions)]
            _debug_loader: debug_loader,
            #[cfg(debug_assertions)]
            _debug_messenger: debug_messenger,
            vsync_enabled: false,
        };

        r.create_swapchain(window, None)?;
        r.create_render_pass()?;
        r.create_command_pool()?;
        r.create_depth_resources()?;
        r.create_framebuffers()?;
        r.load_mesh_and_pipeline()?;
        r.create_uniforms_and_descriptors()?;
        r.create_sync()?;
        r.allocate_command_buffers()?;

        Ok(r)
    }

    /// `inner_size_override`: use when the OS has not yet updated [`Window::inner_size`] after
    /// `request_inner_size` (common on Windows for child windows).
    fn create_swapchain(
        &mut self,
        window: &Window,
        inner_size_override: Option<(u32, u32)>,
    ) -> Result<(), RenderError> {
        let caps = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface)?
        };
        let formats = unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(self.physical_device, self.surface)?
        };
        let present_modes = unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(self.physical_device, self.surface)?
        };

        let format = formats
            .iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_SRGB
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| formats.first())
            .ok_or_else(|| RenderError::Msg("no surface formats".into()))?;

        let present_mode = if self.vsync_enabled {
            vk::PresentModeKHR::FIFO
        } else if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
            vk::PresentModeKHR::IMMEDIATE
        } else {
            vk::PresentModeKHR::FIFO
        };

        let sz = window.inner_size();
        let (iw, ih, force_inner) = match inner_size_override {
            Some((w, h)) if w > 0 && h > 0 => (w, h, true),
            _ => (sz.width, sz.height, false),
        };
        let extent = pick_swapchain_extent(&caps, iw, ih, force_inner);

        let mut image_count = caps.min_image_count.saturating_add(1);
        if caps.max_image_count > 0 {
            image_count = image_count.min(caps.max_image_count);
        }

        let mut create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true);

        let families = [self.graphics_family, self.present_family];
        if self.graphics_family != self.present_family {
            create_info = create_info
                .image_sharing_mode(vk::SharingMode::CONCURRENT)
                .queue_family_indices(&families);
        }

        let swapchain = unsafe { self.swapchain_loader.create_swapchain(&create_info, None)? };
        let images = unsafe { self.swapchain_loader.get_swapchain_images(swapchain)? };

        let views: Result<Vec<_>, _> = images
            .iter()
            .map(|&img| {
                let view_info = vk::ImageViewCreateInfo::default()
                    .image(img)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format.format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    });
                unsafe { self.device.create_image_view(&view_info, None) }
            })
            .collect();

        self.destroy_swapchain_chain();

        self.swapchain = swapchain;
        self.swapchain_format = format.format;
        self.swapchain_extent = extent;
        self.swapchain_images = images;
        self.swapchain_views = views?;

        Ok(())
    }

    /// # Safety
    /// Same requirements as [`Self::resize`]. Recreates swapchain-dependent resources when changed.
    pub unsafe fn set_vsync_enabled(
        &mut self,
        window: &Window,
        enabled: bool,
    ) -> Result<(), RenderError> {
        if self.vsync_enabled == enabled {
            return Ok(());
        }
        self.vsync_enabled = enabled;
        self.resize(window)
    }

    fn destroy_swapchain_chain(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
            for &fb in &self.framebuffers {
                if !fb.is_null() {
                    self.device.destroy_framebuffer(fb, None);
                }
            }
            self.framebuffers.clear();
            for &v in &self.swapchain_views {
                if !v.is_null() {
                    self.device.destroy_image_view(v, None);
                }
            }
            self.swapchain_views.clear();
            if !self.swapchain.is_null() {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
        }
    }

    fn create_render_pass(&mut self) -> Result<(), RenderError> {
        if !self.render_pass.is_null() {
            unsafe {
                self.device.destroy_render_pass(self.render_pass, None);
            }
        }
        let color_attachment = vk::AttachmentDescription::default()
            .format(self.swapchain_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

        let depth_attachment = vk::AttachmentDescription::default()
            .format(vk::Format::D32_SFLOAT)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);

        let color_ref = vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };
        let depth_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };

        let subpass = vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(slice::from_ref(&color_ref))
            .depth_stencil_attachment(&depth_ref);

        let deps = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            )];

        let attachments = [color_attachment, depth_attachment];
        let rp_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(slice::from_ref(&subpass))
            .dependencies(&deps);

        self.render_pass = unsafe { self.device.create_render_pass(&rp_info, None)? };
        Ok(())
    }

    fn create_depth_resources(&mut self) -> Result<(), RenderError> {
        unsafe {
            if !self.depth_view.is_null() {
                self.device.destroy_image_view(self.depth_view, None);
                self.depth_view = vk::ImageView::null();
            }
            if !self.depth_image.is_null() {
                self.device.destroy_image(self.depth_image, None);
                self.depth_image = vk::Image::null();
            }
            if !self.depth_mem.is_null() {
                self.device.free_memory(self.depth_mem, None);
                self.depth_mem = vk::DeviceMemory::null();
            }
        }

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::D32_SFLOAT)
            .extent(vk::Extent3D {
                width: self.swapchain_extent.width,
                height: self.swapchain_extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        self.depth_image = unsafe { self.device.create_image(&image_info, None)? };
        let mem_req = unsafe { self.device.get_image_memory_requirements(self.depth_image) };
        let mem_type = find_memory_type(
            mem_req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            self.physical_device,
            &self.instance,
        )?;
        let alloc = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_req.size)
            .memory_type_index(mem_type);
        self.depth_mem = unsafe { self.device.allocate_memory(&alloc, None)? };
        unsafe {
            self.device
                .bind_image_memory(self.depth_image, self.depth_mem, 0)?;
        }

        let view_info = vk::ImageViewCreateInfo::default()
            .image(self.depth_image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::D32_SFLOAT)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });
        self.depth_view = unsafe { self.device.create_image_view(&view_info, None)? };

        transition_image_layout(
            &self.device,
            self.graphics_queue,
            self.command_pool,
            self.depth_image,
            vk::Format::D32_SFLOAT,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        )?;

        Ok(())
    }

    fn create_framebuffers(&mut self) -> Result<(), RenderError> {
        unsafe {
            for &fb in &self.framebuffers {
                self.device.destroy_framebuffer(fb, None);
            }
        }
        self.framebuffers.clear();

        for &view in &self.swapchain_views {
            let attachments = [view, self.depth_view];
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(self.render_pass)
                .attachments(&attachments)
                .width(self.swapchain_extent.width)
                .height(self.swapchain_extent.height)
                .layers(1);
            self.framebuffers
                .push(unsafe { self.device.create_framebuffer(&fb_info, None)? });
        }
        Ok(())
    }

    fn create_command_pool(&mut self) -> Result<(), RenderError> {
        if !self.command_pool.is_null() {
            unsafe {
                self.device.destroy_command_pool(self.command_pool, None);
            }
        }
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(self.graphics_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        self.command_pool = unsafe { self.device.create_command_pool(&pool_info, None)? };
        Ok(())
    }

    fn load_mesh_and_pipeline(&mut self) -> Result<(), RenderError> {
        unsafe {
            if !self.vertex_buffer.is_null() {
                self.device.destroy_buffer(self.vertex_buffer, None);
            }
            if !self.vertex_mem.is_null() {
                self.device.free_memory(self.vertex_mem, None);
            }
            if !self.index_buffer.is_null() {
                self.device.destroy_buffer(self.index_buffer, None);
            }
            if !self.index_mem.is_null() {
                self.device.free_memory(self.index_mem, None);
            }
            if !self.instance_buffer.is_null() {
                self.device.destroy_buffer(self.instance_buffer, None);
            }
            if !self.instance_mem.is_null() {
                self.device.free_memory(self.instance_mem, None);
            }
        }
        self.vertex_buffer = vk::Buffer::null();
        self.vertex_mem = vk::DeviceMemory::null();
        self.index_buffer = vk::Buffer::null();
        self.index_mem = vk::DeviceMemory::null();
        self.instance_buffer = vk::Buffer::null();
        self.instance_mem = vk::DeviceMemory::null();

        let (verts, idx) = cube_vertices();
        self.index_count = idx.len() as u32;

        let v_size = (verts.len() * size_of::<Vertex>()) as u64;
        let i_size = (idx.len() * size_of::<u32>()) as u64;
        let inst_size = (MAX_INSTANCES * size_of::<[f32; 6]>()) as u64;

        let (vb, vm) = create_buffer(
            &self.device,
            self.physical_device,
            &self.instance,
            v_size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe {
            let ptr = self
                .device
                .map_memory(vm, 0, v_size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(verts.as_ptr(), ptr as *mut Vertex, verts.len());
            self.device.unmap_memory(vm);
        }

        let (ib, im) = create_buffer(
            &self.device,
            self.physical_device,
            &self.instance,
            i_size,
            vk::BufferUsageFlags::INDEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        unsafe {
            let ptr = self
                .device
                .map_memory(im, 0, i_size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(idx.as_ptr(), ptr as *mut u32, idx.len());
            self.device.unmap_memory(im);
        }

        let (inst_b, inst_m) = create_buffer(
            &self.device,
            self.physical_device,
            &self.instance,
            inst_size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        self.vertex_buffer = vb;
        self.vertex_mem = vm;
        self.index_buffer = ib;
        self.index_mem = im;
        self.instance_buffer = inst_b;
        self.instance_mem = inst_m;

        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .stage_flags(vk::ShaderStageFlags::VERTEX)];
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        if !self.descriptor_layout.is_null() {
            unsafe {
                self.device
                    .destroy_descriptor_set_layout(self.descriptor_layout, None);
            }
        }
        self.descriptor_layout = unsafe {
            self.device
                .create_descriptor_set_layout(&layout_info, None)?
        };

        let push_ranges = [];
        let set_layouts = [self.descriptor_layout];
        let pl_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts)
            .push_constant_ranges(&push_ranges);
        if !self.pipeline_layout.is_null() {
            unsafe {
                self.device
                    .destroy_pipeline_layout(self.pipeline_layout, None);
            }
        }
        self.pipeline_layout = unsafe { self.device.create_pipeline_layout(&pl_info, None)? };

        let shader_module = create_shader_module(&self.device, MESH_SPV)?;

        let entry_vs = c"vs_main";
        let entry_fs = c"fs_main";

        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(shader_module)
                .name(entry_vs),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(shader_module)
                .name(entry_fs),
        ];

        let binding_descs = [
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: size_of::<Vertex>() as u32,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: (size_of::<f32>() * 6) as u32,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ];
        let attr_descs = [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: 12,
            },
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 1,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                location: 3,
                binding: 1,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: 12,
            },
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&binding_descs)
            .vertex_attribute_descriptions(&attr_descs);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: self.swapchain_extent.width as f32,
            height: self.swapchain_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: self.swapchain_extent,
        };
        let viewports = [viewport];
        let scissors = [scissor];
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);

        let disable_cull = debug_disable_backface_cull();
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            // Default to back-face culling for normal rendering performance.
            // Set VGE_DISABLE_BACKFACE_CULL=1 only when debugging winding/visibility.
            .cull_mode(if disable_cull {
                vk::CullModeFlags::NONE
            } else {
                vk::CullModeFlags::BACK
            })
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS);

        let color_blend_attach = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA);
        let attachments = [color_blend_attach];
        let color_blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic)
            .layout(self.pipeline_layout)
            .render_pass(self.render_pass)
            .subpass(0);

        if !self.pipeline.is_null() {
            unsafe {
                self.device.destroy_pipeline(self.pipeline, None);
            }
        }
        let pipelines = unsafe {
            self.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|(_, e)| e)?
        };
        self.pipeline = pipelines[0];

        unsafe {
            self.device.destroy_shader_module(shader_module, None);
        }

        Ok(())
    }

    fn create_uniforms_and_descriptors(&mut self) -> Result<(), RenderError> {
        for &b in &self.uniform_buffers {
            if !b.is_null() {
                unsafe { self.device.destroy_buffer(b, None) };
            }
        }
        for &m in &self.uniform_memories {
            if !m.is_null() {
                unsafe { self.device.free_memory(m, None) };
            }
        }
        self.uniform_buffers.clear();
        self.uniform_memories.clear();
        if !self.descriptor_pool.is_null() {
            unsafe {
                self.device
                    .destroy_descriptor_pool(self.descriptor_pool, None);
            }
        }

        let ubo_size = size_of::<GlobalsUbo>() as u64;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let (b, mem) = create_buffer(
                &self.device,
                self.physical_device,
                &self.instance,
                ubo_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            self.uniform_buffers.push(b);
            self.uniform_memories.push(mem);
        }

        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
        }];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32);
        self.descriptor_pool = unsafe { self.device.create_descriptor_pool(&pool_info, None)? };

        let layouts = vec![self.descriptor_layout; MAX_FRAMES_IN_FLIGHT];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&layouts);
        self.descriptor_sets = unsafe { self.device.allocate_descriptor_sets(&alloc_info)? };

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let info = vk::DescriptorBufferInfo::default()
                .buffer(self.uniform_buffers[i])
                .offset(0)
                .range(ubo_size);
            let write = vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_sets[i])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(slice::from_ref(&info));
            unsafe {
                self.device
                    .update_descriptor_sets(slice::from_ref(&write), &[])
            };
        }

        Ok(())
    }

    fn create_sync(&mut self) -> Result<(), RenderError> {
        unsafe {
            for &s in &self.image_available {
                self.device.destroy_semaphore(s, None);
            }
            for &s in &self.render_finished {
                self.device.destroy_semaphore(s, None);
            }
            for &f in &self.in_flight {
                self.device.destroy_fence(f, None);
            }
        }
        self.image_available.clear();
        self.render_finished.clear();
        self.in_flight.clear();

        let sem_info = vk::SemaphoreCreateInfo::default();
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            unsafe {
                self.image_available
                    .push(self.device.create_semaphore(&sem_info, None)?);
                self.in_flight
                    .push(self.device.create_fence(&fence_info, None)?);
            }
        }
        for _ in 0..self.swapchain_images.len() {
            unsafe {
                self.render_finished
                    .push(self.device.create_semaphore(&sem_info, None)?);
            }
        }
        Ok(())
    }

    fn allocate_command_buffers(&mut self) -> Result<(), RenderError> {
        if !self.command_buffers.is_empty() {
            unsafe {
                self.device
                    .free_command_buffers(self.command_pool, &self.command_buffers);
            }
            self.command_buffers.clear();
        }
        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(MAX_FRAMES_IN_FLIGHT as u32);
        self.command_buffers = unsafe { self.device.allocate_command_buffers(&alloc_info)? };
        Ok(())
    }

    /// # Safety
    /// Same requirements as [`Self::new`] for `window`, and no concurrent use of this renderer on another thread.
    pub unsafe fn resize(&mut self, window: &Window) -> Result<(), RenderError> {
        let s = window.inner_size();
        if s.width == 0 || s.height == 0 {
            return Ok(());
        }
        self.device.device_wait_idle()?;
        self.destroy_swapchain_chain();
        self.create_swapchain(window, None)?;
        self.create_render_pass()?;
        self.create_depth_resources()?;
        self.load_mesh_and_pipeline()?;
        self.create_framebuffers()?;
        self.create_uniforms_and_descriptors()?;
        self.create_sync()?;
        self.allocate_command_buffers()?;
        Ok(())
    }

    /// Like [`Self::resize`], but uses `width` / `height` for the swapchain extent instead of
    /// [`Window::inner_size`], which may lag behind [`Window::request_inner_size`] on some platforms.
    ///
    /// # Safety
    /// Same as [`Self::resize`].
    pub unsafe fn resize_to(
        &mut self,
        window: &Window,
        width: u32,
        height: u32,
    ) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.device.device_wait_idle()?;
        self.destroy_swapchain_chain();
        self.create_swapchain(window, Some((width, height)))?;
        self.create_render_pass()?;
        self.create_depth_resources()?;
        self.load_mesh_and_pipeline()?;
        self.create_framebuffers()?;
        self.create_uniforms_and_descriptors()?;
        self.create_sync()?;
        self.allocate_command_buffers()?;
        Ok(())
    }

    /// # Safety
    /// Vulkan handles on `self` must remain valid; caller must not destroy the swapchain or device concurrently.
    pub unsafe fn draw_frame(
        &mut self,
        instance_data: &[[f32; 6]],
        view_proj: Mat4,
    ) -> Result<(), RenderError> {
        let frame = self.current_frame;
        self.device
            .wait_for_fences(slice::from_ref(&self.in_flight[frame]), true, u64::MAX)?;
        self.device
            .reset_fences(slice::from_ref(&self.in_flight[frame]))?;

        let (image_index, _) = match unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available[frame],
                vk::Fence::null(),
            )
        } {
            Ok(x) => x,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                return Err(RenderError::Vulkan(vk::Result::ERROR_OUT_OF_DATE_KHR));
            }
            Err(e) => return Err(e.into()),
        };
        let image_index = image_index as usize;

        let inst_n = instance_data.len().min(MAX_INSTANCES);
        let inst_bytes = inst_n * size_of::<[f32; 6]>();
        if inst_n > 0 {
            let ptr = self.device.map_memory(
                self.instance_mem,
                0,
                inst_bytes as u64,
                vk::MemoryMapFlags::empty(),
            )?;
            std::ptr::copy_nonoverlapping(
                instance_data.as_ptr(),
                ptr as *mut [f32; 6],
                inst_n,
            );
            self.device.unmap_memory(self.instance_mem);
        }

        let globals = GlobalsUbo {
            view_proj: view_proj.to_cols_array_2d(),
        };
        let ubo_size = size_of::<GlobalsUbo>() as u64;
        let uptr = self.device.map_memory(
            self.uniform_memories[frame],
            0,
            ubo_size,
            vk::MemoryMapFlags::empty(),
        )?;
        std::ptr::write(uptr as *mut GlobalsUbo, globals);
        self.device.unmap_memory(self.uniform_memories[frame]);

        self.device.reset_command_buffer(
            self.command_buffers[frame],
            vk::CommandBufferResetFlags::empty(),
        )?;

        let cmd = self.command_buffers[frame];
        let cmd_begin = vk::CommandBufferBeginInfo::default();
        self.device.begin_command_buffer(cmd, &cmd_begin)?;

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.05, 0.06, 0.1, 1.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        let rp_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: self.swapchain_extent,
            })
            .clear_values(&clear_values);

        self.device
            .cmd_begin_render_pass(cmd, &rp_info, vk::SubpassContents::INLINE);
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

        let vp = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: self.swapchain_extent.width as f32,
            height: self.swapchain_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let sc = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: self.swapchain_extent,
        };
        self.device.cmd_set_viewport(cmd, 0, slice::from_ref(&vp));
        self.device.cmd_set_scissor(cmd, 0, slice::from_ref(&sc));

        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipeline_layout,
            0,
            slice::from_ref(&self.descriptor_sets[frame]),
            &[],
        );

        let vb = [self.vertex_buffer];
        let offs = [0u64, 0u64];
        self.device
            .cmd_bind_vertex_buffers(cmd, 0, &vb, slice::from_ref(&offs[0]));
        let ibufs = [self.instance_buffer];
        self.device
            .cmd_bind_vertex_buffers(cmd, 1, &ibufs, slice::from_ref(&offs[1]));
        self.device
            .cmd_bind_index_buffer(cmd, self.index_buffer, 0, vk::IndexType::UINT32);

        self.device
            .cmd_draw_indexed(cmd, self.index_count, inst_n as u32, 0, 0, 0);

        self.device.cmd_end_render_pass(cmd);
        self.device.end_command_buffer(cmd)?;

        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(slice::from_ref(&self.image_available[frame]))
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(slice::from_ref(&cmd))
            .signal_semaphores(slice::from_ref(&self.render_finished[image_index]));

        self.device.queue_submit(
            self.graphics_queue,
            slice::from_ref(&submit_info),
            self.in_flight[frame],
        )?;

        let swapchains = [self.swapchain];
        let indices = [image_index as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(slice::from_ref(&self.render_finished[image_index]))
            .swapchains(&swapchains)
            .image_indices(&indices);

        match unsafe {
            self.swapchain_loader
                .queue_present(self.present_queue, &present_info)
        } {
            Ok(false) => {}
            Ok(true) => {
                return Err(RenderError::Vulkan(vk::Result::ERROR_OUT_OF_DATE_KHR));
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                return Err(RenderError::Vulkan(vk::Result::ERROR_OUT_OF_DATE_KHR));
            }
            Err(e) => return Err(e.into()),
        }

        self.current_frame = (frame + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(())
    }
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            for &s in &self.image_available {
                self.device.destroy_semaphore(s, None);
            }
            for &s in &self.render_finished {
                self.device.destroy_semaphore(s, None);
            }
            for &f in &self.in_flight {
                self.device.destroy_fence(f, None);
            }
            if !self.descriptor_pool.is_null() {
                self.device
                    .destroy_descriptor_pool(self.descriptor_pool, None);
            }
            if !self.descriptor_layout.is_null() {
                self.device
                    .destroy_descriptor_set_layout(self.descriptor_layout, None);
            }
            for &b in &self.uniform_buffers {
                if !b.is_null() {
                    self.device.destroy_buffer(b, None);
                }
            }
            for &m in &self.uniform_memories {
                if !m.is_null() {
                    self.device.free_memory(m, None);
                }
            }
            if !self.pipeline.is_null() {
                self.device.destroy_pipeline(self.pipeline, None);
            }
            if !self.pipeline_layout.is_null() {
                self.device
                    .destroy_pipeline_layout(self.pipeline_layout, None);
            }
            if !self.vertex_buffer.is_null() {
                self.device.destroy_buffer(self.vertex_buffer, None);
            }
            if !self.vertex_mem.is_null() {
                self.device.free_memory(self.vertex_mem, None);
            }
            if !self.index_buffer.is_null() {
                self.device.destroy_buffer(self.index_buffer, None);
            }
            if !self.index_mem.is_null() {
                self.device.free_memory(self.index_mem, None);
            }
            if !self.instance_buffer.is_null() {
                self.device.destroy_buffer(self.instance_buffer, None);
            }
            if !self.instance_mem.is_null() {
                self.device.free_memory(self.instance_mem, None);
            }
            for &fb in &self.framebuffers {
                if !fb.is_null() {
                    self.device.destroy_framebuffer(fb, None);
                }
            }
            if !self.command_pool.is_null() {
                self.device.destroy_command_pool(self.command_pool, None);
            }
            if !self.render_pass.is_null() {
                self.device.destroy_render_pass(self.render_pass, None);
            }
            for &v in &self.swapchain_views {
                if !v.is_null() {
                    self.device.destroy_image_view(v, None);
                }
            }
            if !self.depth_view.is_null() {
                self.device.destroy_image_view(self.depth_view, None);
            }
            if !self.depth_image.is_null() {
                self.device.destroy_image(self.depth_image, None);
            }
            if !self.depth_mem.is_null() {
                self.device.free_memory(self.depth_mem, None);
            }
            if !self.swapchain.is_null() {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None);
            }
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            #[cfg(debug_assertions)]
            self._debug_loader
                .destroy_debug_utils_messenger(self._debug_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let msg = unsafe { CStr::from_ptr((*data).p_message) };
    match severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => error!("validation: {:?}", msg),
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => warn!("validation: {:?}", msg),
        _ => info!("validation: {:?}", msg),
    }
    vk::FALSE
}

fn pick_physical_device(
    instance: &Instance,
    surface_loader: &surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<vk::PhysicalDevice, RenderError> {
    let pds = unsafe { instance.enumerate_physical_devices()? };
    for &pd in &pds {
        let props = unsafe { instance.get_physical_device_properties(pd) };
        if (props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
            || props.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU)
            && queue_families_ok(instance, pd, surface_loader, surface)
        {
            return Ok(pd);
        }
    }
    for &pd in &pds {
        if queue_families_ok(instance, pd, surface_loader, surface) {
            return Ok(pd);
        }
    }
    Err(RenderError::Msg("no suitable GPU".into()))
}

fn queue_families_ok(
    instance: &Instance,
    pd: vk::PhysicalDevice,
    surface_loader: &surface::Instance,
    surface: vk::SurfaceKHR,
) -> bool {
    find_queue_families(instance, pd, surface_loader, surface).is_ok()
}

fn find_queue_families(
    instance: &Instance,
    pd: vk::PhysicalDevice,
    surface_loader: &surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(u32, u32), RenderError> {
    let props = unsafe { instance.get_physical_device_queue_family_properties(pd) };
    let mut graphics = None;
    let mut present = None;
    for (i, p) in props.iter().enumerate() {
        let i = i as u32;
        if p.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            graphics.get_or_insert(i);
        }
        let ok = unsafe { surface_loader.get_physical_device_surface_support(pd, i, surface)? };
        if ok {
            present.get_or_insert(i);
        }
    }
    match (graphics, present) {
        (Some(g), Some(p)) => Ok((g, p)),
        _ => Err(RenderError::Msg("missing queue family".into())),
    }
}

fn find_memory_type(
    type_filter: u32,
    props: vk::MemoryPropertyFlags,
    pd: vk::PhysicalDevice,
    instance: &Instance,
) -> Result<u32, RenderError> {
    let mem_props = unsafe { instance.get_physical_device_memory_properties(pd) };
    for i in 0..mem_props.memory_type_count {
        if (type_filter & (1 << i)) != 0
            && (mem_props.memory_types[i as usize].property_flags & props) == props
        {
            return Ok(i);
        }
    }
    Err(RenderError::Msg("no memory type".into()))
}

fn create_buffer(
    device: &Device,
    pd: vk::PhysicalDevice,
    instance: &Instance,
    size: u64,
    usage: vk::BufferUsageFlags,
    mem_props: vk::MemoryPropertyFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory), RenderError> {
    let info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&info, None)? };
    let req = unsafe { device.get_buffer_memory_requirements(buffer) };
    let ty = find_memory_type(req.memory_type_bits, mem_props, pd, instance)?;
    let alloc = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(ty);
    let mem = unsafe { device.allocate_memory(&alloc, None)? };
    unsafe {
        device.bind_buffer_memory(buffer, mem, 0)?;
    }
    Ok((buffer, mem))
}

fn create_shader_module(device: &Device, code: &[u8]) -> Result<vk::ShaderModule, RenderError> {
    if code.len() % 4 != 0 {
        return Err(RenderError::Msg(
            "shader bytecode length must be a multiple of 4".into(),
        ));
    }

    // SPIR-V is a u32 word stream. `include_bytes!` does not guarantee 4-byte alignment,
    // so avoid `cast_slice` and decode into words explicitly.
    let words: Vec<u32> = code
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    let info = vk::ShaderModuleCreateInfo::default().code(&words);
    Ok(unsafe { device.create_shader_module(&info, None)? })
}

fn transition_image_layout(
    device: &Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    image: vk::Image,
    _format: vk::Format,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
) -> Result<(), RenderError> {
    let cmd_alloc = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let cmd = unsafe { device.allocate_command_buffers(&cmd_alloc)? }[0];
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    unsafe {
        device.begin_command_buffer(cmd, &begin)?;
    }

    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(old)
        .new_layout(new)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );

    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            slice::from_ref(&barrier),
        );
        device.end_command_buffer(cmd)?;
    }

    let submit = vk::SubmitInfo::default().command_buffers(slice::from_ref(&cmd));
    unsafe {
        device.queue_submit(queue, slice::from_ref(&submit), vk::Fence::null())?;
        device.queue_wait_idle(queue)?;
        device.free_command_buffers(pool, slice::from_ref(&cmd));
    }
    Ok(())
}
