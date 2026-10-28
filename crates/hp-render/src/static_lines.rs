use crate::overlay::{self, Chunk, OverlayLayout};
use crate::{Overlay, OverlayLine, Renderer, StaticLineStyle};

pub(crate) struct StaticLayer {
    pub buffer: wgpu::Buffer,
    pub count: u32,
    pub chunks: Vec<Chunk>,
    pub style: StaticLineStyle,
    width: wgpu::Buffer,
    pub width_bind: wgpu::BindGroup,
}

fn clean(style: StaticLineStyle) -> StaticLineStyle {
    let width_px = if style.width_px.is_finite() { style.width_px.clamp(1.0, 32.0) } else { 1.0 };
    StaticLineStyle { width_px, ..style }
}

impl Renderer {
    pub fn set_static_lines(&mut self, layer: u32, lines: &[OverlayLine]) {
        let style = self.static_lines.remove(&layer).map_or_else(StaticLineStyle::default, |l| l.style);
        self.content_gen += 1;
        if lines.is_empty() {
            return;
        }
        let overlay = Overlay { lines: lines.to_vec(), ..Default::default() };
        let Some((layout, total)) = OverlayLayout::of(&overlay).filter(|(_, total)| *total <= self.max_buffer) else {
            log::warn!("hp-render: static line layer {layer} too large, skipped");
            return;
        };
        let size = total.max(16).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render static lines"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut offset = 0u64;
        let queue = &self.queue;
        let mut scratch = Vec::new();
        overlay::encode(&overlay, &mut scratch, overlay::UPLOAD_BLOCK, &mut |bytes| {
            queue.write_buffer(&buffer, offset, bytes);
            offset += bytes.len() as u64;
        });
        let [chunks, ..] = overlay::chunks(&overlay, &layout);
        let width = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render static line width"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let width_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render static line width"),
            layout: &self.shared.wide_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: width.as_entire_binding() }],
        });
        let style = clean(style);
        self.queue.write_buffer(&width, 0, bytemuck::cast_slice(&[style.width_px, 0.0, 0.0, 0.0]));
        let count = layout.lines.1 / 2;
        self.static_lines.insert(layer, StaticLayer { buffer, count, chunks, style, width, width_bind });
    }

    pub fn set_static_lines_style(&mut self, layer: u32, style: StaticLineStyle) {
        let Some(l) = self.static_lines.get_mut(&layer) else { return };
        let style = clean(style);
        if l.style == style {
            return;
        }
        l.style = style;
        self.queue.write_buffer(&l.width, 0, bytemuck::cast_slice(&[style.width_px, 0.0, 0.0, 0.0]));
        self.content_gen += 1;
    }

    pub fn static_lines_style(&self, layer: u32) -> Option<StaticLineStyle> {
        self.static_lines.get(&layer).map(|l| l.style)
    }

    pub fn static_lines_count(&self, layer: u32) -> u32 {
        self.static_lines.get(&layer).map_or(0, |l| l.count)
    }

    pub fn clear_static_lines(&mut self, layer: u32) {
        if self.static_lines.remove(&layer).is_some() {
            self.content_gen += 1;
        }
    }
}
