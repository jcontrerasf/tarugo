//! Draws the 3D scene with wgpu into an offscreen texture (multisampled color + depth) that
//! egui then shows as an image. Dimensions, handles and marks are painted over it by egui.

use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use super::mesh::{Mesh, Vertex};

const SAMPLES: u32 = 4;
/// egui samples its textures as gamma-encoded values, so the scene is rendered in gamma
/// space into a plain (non-sRGB) target.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const SHADER: &str = r"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs(v: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    var shade = 1.0;
    if (dot(v.normal, v.normal) > 0.0) {
        // Half Lambert: no face goes black, and each one gets its own tone.
        let d = dot(normalize(v.normal), u.light.xyz) * 0.5 + 0.5;
        shade = 0.45 + 0.6 * d;
    }
    out.color = vec4<f32>(v.color.rgb * shade, v.color.a);
    return out;
}

@fragment
fn fs(v: VertexOut) -> @location(0) vec4<f32> {
    return v.color;
}
";

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light: [f32; 4],
}

struct Targets {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved: wgpu::TextureView,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    egui: Arc<egui::mutex::RwLock<egui_wgpu::Renderer>>,
    triangles: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    targets: Option<Targets>,
    texture: Option<egui::TextureId>,
    triangle_buffer: Option<(wgpu::Buffer, u32)>,
    line_buffer: Option<(wgpu::Buffer, u32)>,
}

impl Renderer {
    pub fn new(state: &egui_wgpu::RenderState) -> Renderer {
        let device = state.device.clone();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tarugo scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tarugo uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tarugo uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tarugo uniforms"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tarugo scene"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
        let pipeline = |topology: wgpu::PrimitiveTopology, bias: wgpu::DepthBiasState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("tarugo scene"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // Faces are pushed back a little so the outline drawn on their edges wins.
        let triangles = pipeline(
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::DepthBiasState {
                constant: 4,
                slope_scale: 1.5,
                clamp: 0.0,
            },
        );
        let lines = pipeline(wgpu::PrimitiveTopology::LineList, Default::default());
        Renderer {
            device,
            queue: state.queue.clone(),
            egui: state.renderer.clone(),
            triangles,
            lines,
            uniforms,
            bind_group,
            targets: None,
            texture: None,
            triangle_buffer: None,
            line_buffer: None,
        }
    }

    pub fn set_mesh(&mut self, mesh: &Mesh) {
        let buffer = |vertices: &[Vertex]| {
            (!vertices.is_empty()).then(|| {
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("tarugo vertices"),
                        contents: bytemuck::cast_slice(vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                (buffer, vertices.len() as u32)
            })
        };
        self.triangle_buffer = buffer(&mesh.triangles);
        self.line_buffer = buffer(&mesh.lines);
    }

    fn ensure_targets(&mut self, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |label, format, samples, usage| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let targets = Targets {
            size,
            color: texture("tarugo color", COLOR_FORMAT, SAMPLES, attachment),
            depth: texture("tarugo depth", DEPTH_FORMAT, SAMPLES, attachment),
            resolved: texture(
                "tarugo resolved",
                COLOR_FORMAT,
                1,
                attachment | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
        };
        let mut egui = self.egui.write();
        match self.texture {
            Some(id) => egui.update_egui_texture_from_wgpu_texture(
                &self.device,
                &targets.resolved,
                wgpu::FilterMode::Linear,
                id,
            ),
            None => {
                self.texture = Some(egui.register_native_texture(
                    &self.device,
                    &targets.resolved,
                    wgpu::FilterMode::Linear,
                ));
            }
        }
        drop(egui);
        self.targets = Some(targets);
    }

    /// Renders the scene at `size` pixels and returns the texture to show. `light` is the
    /// direction towards the light; `clear` the background color.
    pub fn render(
        &mut self,
        size: [u32; 2],
        view_proj: Mat4,
        light: Vec3,
        clear: [f32; 4],
    ) -> Option<egui::TextureId> {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        self.ensure_targets(size);
        let targets = self.targets.as_ref()?;
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light: light.normalize_or_zero().extend(0.0).to_array(),
        };
        self.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("tarugo scene"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tarugo scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.color,
                    depth_slice: None,
                    resolve_target: Some(&targets.resolved),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0] as f64,
                            g: clear[1] as f64,
                            b: clear[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            for (pipeline, buffer) in [
                (&self.triangles, &self.triangle_buffer),
                (&self.lines, &self.line_buffer),
            ] {
                if let Some((buffer, count)) = buffer {
                    pass.set_pipeline(pipeline);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..*count, 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        self.texture
    }
}
