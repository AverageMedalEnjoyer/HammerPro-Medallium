use std::collections::HashMap;

use crate::hooks::HookContext;
use crate::pipelines::COLOR_FORMAT;

pub(crate) const BLOOM_SCALE: f32 = 1.8;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

struct Pipes {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    down: wgpu::RenderPipeline,
    blur_x: wgpu::RenderPipeline,
    blur_y: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
}

struct Targets {
    size: [u32; 2],
    down: wgpu::TextureView,
    across: wgpu::TextureView,
}

#[derive(Default)]
pub(crate) struct Bloom {
    pipes: Option<Pipes>,
    targets: HashMap<u32, Targets>,
}

fn pipes(device: &wgpu::Device) -> Pipes {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hp-render bloom"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/bloom.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hp-render bloom"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hp-render bloom"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let make = |fs: &str, format: wgpu::TextureFormat, blur_y: bool, additive: bool| {
        let constants = [("BLUR_Y", if blur_y { 1.0 } else { 0.0 }), ("BLOOM_SCALE", f64::from(BLOOM_SCALE))];
        let options =
            wgpu::PipelineCompilationOptions { constants: &constants, zero_initialize_workgroup_memory: false };
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let keep = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hp-render bloom"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_full"),
                compilation_options: options.clone(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(fs),
                compilation_options: options,
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: additive.then_some(wgpu::BlendState { color: add, alpha: keep }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("hp-render bloom"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    Pipes {
        down: make("fs_down", FORMAT, false, false),
        blur_x: make("fs_blur", FORMAT, false, false),
        blur_y: make("fs_blur", FORMAT, true, false),
        add: make("fs_add", COLOR_FORMAT, false, true),
        layout,
        sampler,
    }
}

fn target(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render bloom"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn pass(
    enc: &mut wgpu::CommandEncoder,
    device: &wgpu::Device,
    p: &Pipes,
    pipeline: &wgpu::RenderPipeline,
    src: &wgpu::TextureView,
    dst: &wgpu::TextureView,
    load: bool,
) {
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hp-render bloom"),
        layout: &p.layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&p.sampler) },
        ],
    });
    let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("hp-render bloom"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: dst,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: if load { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) },
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    rp.set_pipeline(pipeline);
    rp.set_bind_group(0, &bg, &[]);
    rp.draw(0..3, 0..1);
}

impl Bloom {
    pub(crate) fn draw(&mut self, cx: &HookContext<'_>, enc: &mut wgpu::CommandEncoder) {
        let device = cx.device();
        let size = [cx.size[0].div_ceil(4).max(1), cx.size[1].div_ceil(4).max(1)];
        let p = self.pipes.get_or_insert_with(|| pipes(device));
        let t = self.targets.entry(cx.view_id).or_insert_with(|| Targets {
            size,
            down: target(device, size),
            across: target(device, size),
        });
        if t.size != size {
            *t = Targets { size, down: target(device, size), across: target(device, size) };
        }
        let source = cx.color_texture.create_view(&wgpu::TextureViewDescriptor::default());
        pass(enc, device, p, &p.down, &source, &t.down, false);
        pass(enc, device, p, &p.blur_x, &t.down, &t.across, false);
        pass(enc, device, p, &p.blur_y, &t.across, &t.down, false);
        pass(enc, device, p, &p.add, &t.down, cx.color, true);
    }
}
