use crate::{Renderer, ViewOptions, ViewParams};

pub trait PassHook: Send + Sync {
    fn name(&self) -> &'static str;

    fn prepare(&mut self, _cx: &HookContext<'_>, _encoder: &mut wgpu::CommandEncoder) {}

    fn draw_opaque(&self, _cx: &HookContext<'_>, _pass: &mut wgpu::RenderPass<'_>) {}

    fn draw_translucent(&self, _cx: &HookContext<'_>, _pass: &mut wgpu::RenderPass<'_>) {}

    fn finish(&mut self, _cx: &HookContext<'_>, _encoder: &mut wgpu::CommandEncoder) {}
}

pub struct HookContext<'a> {
    pub(crate) renderer: &'a Renderer,
    pub view_id: u32,
    pub params: &'a ViewParams,
    pub options: &'a ViewOptions,
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 3],
    pub is_ortho: bool,
    pub size: [u32; 2],
    pub samples: u32,
    pub color: &'a wgpu::TextureView,
    pub color_texture: &'a wgpu::Texture,
    pub view_bind_group: &'a wgpu::BindGroup,
}

impl<'a> HookContext<'a> {
    pub fn renderer(&self) -> &'a Renderer {
        self.renderer
    }

    pub fn device(&self) -> &'a wgpu::Device {
        &self.renderer.device
    }

    pub fn queue(&self) -> &'a wgpu::Queue {
        &self.renderer.queue
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        crate::pipelines::COLOR_FORMAT
    }

    pub fn depth_format(&self) -> wgpu::TextureFormat {
        crate::pipelines::DEPTH_FORMAT
    }
}
