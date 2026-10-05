use anyhow::{Context, Result};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use std::{ffi::c_void, ptr::NonNull};
use wayland_client::{Connection, Proxy};

use crate::source::FrameParams;

pub struct GpuRenderer {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl GpuRenderer {
    pub fn new(conn: &Connection, layer: &LayerSurface, width: u32, height: u32) -> Result<Self> {
        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_desc.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(instance_desc);

        let display = NonNull::new(conn.backend().display_ptr().cast::<c_void>())
            .context("Wayland display pointer is null")?;
        let wl_surface = NonNull::new(layer.wl_surface().id().as_ptr().cast::<c_void>())
            .context("Wayland surface pointer is null")?;

        let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display));
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(wl_surface));

        // SAFETY: the Wayland Connection and LayerSurface are owned by the runtime
        // state and outlive GpuRenderer. GpuRenderer is dropped before either owner.
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
        }
        .context("creating wgpu Wayland surface")?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .context("requesting a GPU adapter compatible with the Wayland surface")?;

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .context("requesting wgpu device")?;

        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .context("surface has no supported default configuration")?;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gnoshow-rear-plane"),
            source: wgpu::ShaderSource::Wgsl(include_str!("rear_plane.wgsl").into()),
        });

        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gnoshow-frame-params-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gnoshow-frame-params"),
            size: std::mem::size_of::<FrameParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gnoshow-frame-params-bind-group"),
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gnoshow-pipeline-layout"),
            bind_group_layouts: &[Some(&params_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gnoshow-rear-plane-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            _instance: instance,
            surface,
            adapter,
            device,
            queue,
            config,
            params_buffer,
            params_bind_group,
            pipeline,
        })
    }

    pub fn adapter_name(&self) -> String {
        self.adapter.get_info().name
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.config.width == width && self.config.height == height {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, params: &FrameParams) -> Result<()> {
        self.queue
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(params));

        let (frame, reconfigure_after_present) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, false),
                    other => anyhow::bail!("surface acquisition after reconfigure: {other:?}"),
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout => anyhow::bail!("surface acquisition timed out"),
            wgpu::CurrentSurfaceTexture::Occluded => anyhow::bail!("surface is occluded"),
            wgpu::CurrentSurfaceTexture::Lost => anyhow::bail!("Wayland GPU surface was lost"),
            wgpu::CurrentSurfaceTexture::Validation => {
                anyhow::bail!("surface acquisition validation failure")
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gnoshow-frame-encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gnoshow-rear-plane-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.params_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);

        if reconfigure_after_present {
            self.surface.configure(&self.device, &self.config);
        }

        Ok(())
    }
}
