use bytemuck::{Pod, Zeroable};

use crate::pipelines::{Blend, Desc, Shared, build, vertex_layout};
use crate::{BackgroundImage, OrthoAxis, Renderer, TexId};

const BG_WGSL: &str = concat!(include_str!("shaders/common.wgsl"), "\n", include_str!("shaders/bgimage.wgsl"));

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct BgVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    alpha: f32,
}

const BG_ATTRS: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 20, shader_location: 2 },
];

pub(crate) struct BgPipelines {
    bgl: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    pub pipelines: [Option<wgpu::RenderPipeline>; 2],
}

pub(crate) struct BgImageGpu {
    pub axis: OrthoAxis,
    pub buffer: wgpu::Buffer,
    pub bind: wgpu::BindGroup,
    texture: TexId,
}

impl BgPipelines {
    fn new(device: &wgpu::Device, shared: &Shared) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render background image"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render background image"),
            bind_group_layouts: &[Some(&shared.view_bgl), Some(&bgl)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render background image"),
            source: wgpu::ShaderSource::Wgsl(BG_WGSL.into()),
        });
        Self { bgl, layout, module, pipelines: [None, None] }
    }

    fn ensure(&mut self, device: &wgpu::Device, samples: u32) {
        let i = usize::from(samples > 1);
        if self.pipelines[i].is_some() {
            return;
        }
        let vb = [Some(vertex_layout(size_of::<BgVertex>(), wgpu::VertexStepMode::Vertex, &BG_ATTRS))];
        self.pipelines[i] = Some(build(
            device,
            samples,
            Desc {
                label: "hp-render background image",
                module: &self.module,
                vs: "vs_bg",
                fs: "fs_bg",
                layout: &self.layout,
                buffers: &vb,
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull: None,
                depth_test: false,
                depth_write: false,
                blend: Blend::Alpha,
                constants: &[],
            },
        ));
    }
}

impl Renderer {
    pub fn set_background_images(&mut self, images: &[BackgroundImage]) {
        self.content_gen += 1;
        self.bg_images.clear();
        if images.is_empty() {
            return;
        }
        if self.bg_pipes.is_none() {
            self.bg_pipes = Some(BgPipelines::new(&self.device, &self.shared));
        }
        for img in images.iter().take(64) {
            let Some(view) = self.textures.get(&img.texture.0).map(|t| &t.view) else { continue };
            if !img.corners.iter().flatten().all(|v| v.is_finite()) {
                continue;
            }
            let alpha = if img.opacity.is_finite() { img.opacity.clamp(0.0, 1.0) } else { 1.0 };
            let [a, b, c, d] = img.corners;
            let v = |pos: [f32; 3], uv: [f32; 2]| BgVertex { pos, uv, alpha };
            let verts = [
                v(a, [0.0, 0.0]),
                v(b, [1.0, 0.0]),
                v(c, [1.0, 1.0]),
                v(a, [0.0, 0.0]),
                v(c, [1.0, 1.0]),
                v(d, [0.0, 1.0]),
            ];
            let data: &[u8] = bytemuck::cast_slice(&verts);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render background image"),
                size: data.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&buffer, 0, data);
            let Some(pipes) = self.bg_pipes.as_ref() else { return };
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render background image"),
                layout: &pipes.bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }],
            });
            self.bg_images.push(BgImageGpu { axis: img.axis, buffer, bind, texture: img.texture });
        }
    }

    pub(crate) fn ensure_bg_pipelines(&mut self, samples: u32) {
        if self.bg_images.is_empty() {
            return;
        }
        if let Some(p) = self.bg_pipes.as_mut() {
            p.ensure(&self.device, samples);
        }
    }

    pub(crate) fn forget_bg_texture(&mut self, id: TexId) {
        self.bg_images.retain(|i| i.texture != id);
    }
}
