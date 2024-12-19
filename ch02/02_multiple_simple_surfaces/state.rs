use bytemuck::cast_slice;
use cgmath::{Matrix, Matrix4, SquareMatrix};
use wgpu::util::DeviceExt;
use winit::{
    event::ElementState, event::KeyEvent, event::WindowEvent, keyboard::Key, keyboard::NamedKey,
    window::Window,
};

use wgpu_surfaces::surface_data as sd;
use wgpu_surfaces::wgpu_simplified as ws;

use crate::vertex::{create_vertices, Vertex};

pub struct State<'a> {
    init: ws::InitWgpu<'a>,
    pipelines: Vec<wgpu::RenderPipeline>,
    vertex_buffers: Vec<wgpu::Buffer>,
    index_buffers: Vec<wgpu::Buffer>,
    uniform_bind_groups: Vec<wgpu::BindGroup>,
    uniform_buffers: Vec<wgpu::Buffer>,
    view_mat: Matrix4<f32>,
    project_mat: Matrix4<f32>,
    msaa_texture_view: wgpu::TextureView,
    depth_texture_view: wgpu::TextureView,
    indices_lens: Vec<u32>,
    plot_type: u32,
    recreate_buffers: bool,
    animation_speed: f32,
    rotation_speed: f32,
    
    x_num: u32,
    z_num: u32,
    objects_count: u32,

    simple_surface: sd::ISimpleSurface,
    fps_counter: ws::FpsCounter,
}

impl<'a> State<'a> {
    pub async fn new(
        window: Window,
        sample_count: u32,
        colormap_name: &'a str,
        wireframe_color: &'a str,
    ) -> Self {
        let init = ws::InitWgpu::init_wgpu(window, sample_count).await;

        // Loading Shaders
        let vs_shader = init
            .device
            .create_shader_module(wgpu::include_wgsl!("shader_instance_vert.wgsl"));
        let fs_shader = init
            .device
            .create_shader_module(wgpu::include_wgsl!("../common/directional_frag.wgsl"));

        // uniform data
        let camera_position = (3.0, 4.5, 5.2).into();
        let look_direction = (0.0, 0.0, 0.0).into();
        let up_direction = cgmath::Vector3::unit_y();
        let light_direction = [-0.5f32, -0.5, -0.5];

        let (view_mat, project_mat, vp_mat) = ws::create_vp_mat(
            camera_position,
            look_direction,
            up_direction,
            init.config.width as f32 / init.config.height as f32,
        );

        // create vertex uniform buffers
        let x_num = 100u32;
        let z_num = 100u32;
        let objects_count = x_num * z_num;

        // model_mat and vp_mat will be stored in vertex_uniform_buffer inside the update function
        let vp_uniform_buffer = init.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("View-Projection Uniform Buffer"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        init.queue.write_buffer(
            &vp_uniform_buffer,
            0,
            cast_slice(vp_mat.as_ref() as &[f32; 16]),
        );

        // model storage buffer
        let model_uniform_buffer = init.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Model Uniform Buffer"),
            size: 64 * objects_count as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // normal storage buffer
        let normal_uniform_buffer = init.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Normal Uniform Buffer"),
            size: 64 * objects_count as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // create light uniform buffer. here we set eye_position = camera_position
        let light_uniform_buffer = init.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Light Uniform Buffer"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let eye_position: &[f32; 3] = camera_position.as_ref();
        init.queue.write_buffer(
            &light_uniform_buffer,
            0,
            cast_slice(light_direction.as_ref()),
        );
        init.queue
            .write_buffer(&light_uniform_buffer, 16, cast_slice(eye_position));

        // set specular light color to white
        let specular_color: [f32; 3] = [1.0, 1.0, 1.0];
        init.queue.write_buffer(
            &light_uniform_buffer,
            32,
            cast_slice(specular_color.as_ref()),
        );

        // material uniform buffer
        let material_uniform_buffer = init.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Material Uniform Buffer"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // set default material parameters
        let material = [0.1f32, 0.7, 0.4, 30.0];
        init.queue
            .write_buffer(&material_uniform_buffer, 0, cast_slice(material.as_ref()));

        // uniform bind group for vertex shader
        let (vert_bind_group_layout, vert_bind_group) = ws::create_bind_group_storage(
            &init.device,
            vec![
                wgpu::ShaderStages::VERTEX,
                wgpu::ShaderStages::VERTEX,
                wgpu::ShaderStages::VERTEX,
            ],
            vec![
                wgpu::BufferBindingType::Uniform,
                wgpu::BufferBindingType::Storage { read_only: true },
                wgpu::BufferBindingType::Storage { read_only: true },
            ],
            &[
                vp_uniform_buffer.as_entire_binding(),
                model_uniform_buffer.as_entire_binding(),
                normal_uniform_buffer.as_entire_binding(),
            ],
        );

        let (vert_bind_group_layout2, vert_bind_group2) = ws::create_bind_group_storage(
            &init.device,
            vec![
                wgpu::ShaderStages::VERTEX,
                wgpu::ShaderStages::VERTEX,
                wgpu::ShaderStages::VERTEX,
            ],
            vec![
                wgpu::BufferBindingType::Uniform,
                wgpu::BufferBindingType::Storage { read_only: true },
                wgpu::BufferBindingType::Storage { read_only: true },
            ],
            &[
                vp_uniform_buffer.as_entire_binding(),
                model_uniform_buffer.as_entire_binding(),
                normal_uniform_buffer.as_entire_binding(),
            ],
        );

        // uniform bind group for fragment shader
        let (frag_bind_group_layout, frag_bind_group) = ws::create_bind_group(
            &init.device,
            vec![wgpu::ShaderStages::FRAGMENT, wgpu::ShaderStages::FRAGMENT],
            &[
                light_uniform_buffer.as_entire_binding(),
                material_uniform_buffer.as_entire_binding(),
            ],
        );
        let (frag_bind_group_layout2, frag_bind_group2) = ws::create_bind_group(
            &init.device,
            vec![wgpu::ShaderStages::FRAGMENT, wgpu::ShaderStages::FRAGMENT],
            &[
                light_uniform_buffer.as_entire_binding(),
                material_uniform_buffer.as_entire_binding(),
            ],
        );

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
            // pos, norm, col
        };

        let pipeline_layout = init
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&vert_bind_group_layout, &frag_bind_group_layout],
                push_constant_ranges: &[],
            });

        let mut ppl = ws::IRenderPipeline {
            vs_shader: Some(&vs_shader),
            fs_shader: Some(&fs_shader),
            pipeline_layout: Some(&pipeline_layout),
            vertex_buffer_layout: &[vertex_buffer_layout],
            ..Default::default()
        };
        let pipeline = ppl.new(&init);

        let vertex_buffer_layout2 = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
            // pos, norm, col
        };

        let pipeline_layout2 =
            init.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout 2"),
                    bind_group_layouts: &[&vert_bind_group_layout2, &frag_bind_group_layout2],
                    push_constant_ranges: &[],
                });

        let mut ppl2 = ws::IRenderPipeline {
            topology: wgpu::PrimitiveTopology::LineList,
            vs_shader: Some(&vs_shader),
            fs_shader: Some(&fs_shader),
            pipeline_layout: Some(&pipeline_layout2),
            vertex_buffer_layout: &[vertex_buffer_layout2],
            ..Default::default()
        };
        let pipeline2 = ppl2.new(&init);

        let msaa_texture_view = ws::create_msaa_texture_view(&init);
        let depth_texture_view = ws::create_depth_view(&init);

        let mut ss = sd::ISimpleSurface {
            scale: 0.5,
            colormap_name: colormap_name.to_string(),
            wireframe_color: wireframe_color.to_string(),
            ..Default::default()
        };
        let data = create_vertices(ss.new());

        let vertex_buffer = init
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: cast_slice(&data.0),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        let vertex_buffer2 = init
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer 2"),
                contents: cast_slice(&data.1),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        let index_buffer = init
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(&data.2),
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            });

        let index_buffer2 = init
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer 2"),
                contents: bytemuck::cast_slice(&data.3),
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            });

        Self {
            init,
            pipelines: vec![pipeline, pipeline2],
            vertex_buffers: vec![vertex_buffer, vertex_buffer2],
            index_buffers: vec![index_buffer, index_buffer2],
            uniform_bind_groups: vec![
                vert_bind_group,
                frag_bind_group,
                vert_bind_group2,
                frag_bind_group2,
            ],
            uniform_buffers: vec![
                vp_uniform_buffer,
                model_uniform_buffer,
                normal_uniform_buffer,
                light_uniform_buffer,
                material_uniform_buffer,
            ],
            view_mat,
            project_mat,
            msaa_texture_view,
            depth_texture_view,
            indices_lens: vec![data.2.len() as u32, data.3.len() as u32],
            plot_type: 1,
            recreate_buffers: false,
            animation_speed: 1.0,
            rotation_speed: 1.0,

            x_num,
            z_num,
            objects_count,

            simple_surface: ss,
            fps_counter: ws::FpsCounter::default(),
        }
    }

    pub fn window(&self) -> &Window {
        &self.init.window
    }

    pub fn size(&self) -> winit::dpi::PhysicalSize<u32> {
        self.init.size
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.init.size = new_size;
            // The surface needs to be reconfigured every time the window is resized.
            self.init.config.width = new_size.width;
            self.init.config.height = new_size.height;
            self.init
                .surface
                .configure(&self.init.device, &self.init.config);

            self.project_mat =
                ws::create_projection_mat(new_size.width as f32 / new_size.height as f32, true);
            self.depth_texture_view = ws::create_depth_view(&self.init);
            if self.init.sample_count > 1 {
                self.msaa_texture_view = ws::create_msaa_texture_view(&self.init);
            }
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: key,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match key.as_ref() {
                Key::Named(NamedKey::Space) => {
                    self.plot_type = (self.plot_type + 1) % 3;
                    return true;
                }
                Key::Named(NamedKey::Control) => {
                    self.simple_surface.surface_type = (self.simple_surface.surface_type + 1) % 3;
                    return true;
                }
                Key::Named(NamedKey::Alt) => {
                    self.simple_surface.colormap_direction =
                        (self.simple_surface.colormap_direction + 1) % 3;
                    return true;
                }
                Key::Character("q") => {
                    self.animation_speed += 0.1;
                    return true;
                }
                Key::Character("a") => {
                    self.animation_speed -= 0.1;
                    if self.animation_speed < 0.0 {
                        self.animation_speed = 0.0;
                    }
                    return true;
                }
                Key::Character("w") => {
                    self.rotation_speed += 0.1;
                    return true;
                }
                Key::Character("s") => {
                    self.rotation_speed -= 0.1;
                    if self.rotation_speed < 0.0 {
                        self.rotation_speed = 0.0;
                    }
                    return true;
                }
                _ => false,
            },
            _ => false,
        }
    }

    pub fn update(&mut self, dt: std::time::Duration) {
        // update uniform buffer
        let mut model_mat: Vec<[f32; 16]> = vec![];
        let mut normal_mat: Vec<[f32; 16]> = vec![];
        let dt1 = self.rotation_speed * dt.as_secs_f32();
        for i in 0..self.x_num {
            for j in 0..self.z_num {
                let translation = [-150.0 + 2.0 * i as f32, 2.0, -180.0 + 2.0 * j as f32];
                let rotation = [
                    (dt1 * i as f32 / self.x_num as f32).sin(),
                    (dt1 * j as f32 / self.z_num as f32).sin(),
                    ((i * j) as f32 * dt1 / self.objects_count as f32).cos(),
                ];
                let scale = [1.0f32, 1.0, 1.0];
                let m = ws::create_model_mat(translation, rotation, scale);
                let n = (m.invert().unwrap()).transpose();
                model_mat.push(*(m.as_ref()));
                normal_mat.push(*(n.as_ref()));
            }
        }
        self.init
            .queue
            .write_buffer(&self.uniform_buffers[1], 0, cast_slice(&model_mat));
        self.init
            .queue
            .write_buffer(&self.uniform_buffers[2], 0, cast_slice(&normal_mat));

        let view_project_mat = self.project_mat * self.view_mat;
        let view_projection_ref: &[f32; 16] = view_project_mat.as_ref();

        self.init.queue.write_buffer(
            &self.uniform_buffers[0],
            0,
            bytemuck::cast_slice(view_projection_ref),
        );

        // recreate vertex and index buffers
        if self.recreate_buffers {
            let data = create_vertices(self.simple_surface.new());
            self.indices_lens = vec![data.2.len() as u32, data.3.len() as u32];
            let vertex_data = [data.0, data.1];
            let index_data = [data.2, data.3];

            for i in 0..2 {
                self.vertex_buffers[i].destroy();
                self.vertex_buffers[i] =
                    self.init
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Vertex Buffer"),
                            contents: cast_slice(&vertex_data[i]),
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        });
                self.index_buffers[i].destroy();
                self.index_buffers[i] =
                    self.init
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Index Buffer"),
                            contents: cast_slice(&index_data[i]),
                            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                        });
            }
            self.recreate_buffers = false;
        }

        // update vertex buffer for every frame
        self.simple_surface.t = self.animation_speed * dt.as_secs_f32();
        let data = create_vertices(self.simple_surface.new());
        self.init
            .queue
            .write_buffer(&self.vertex_buffers[0], 0, cast_slice(&data.0));
        self.init
            .queue
            .write_buffer(&self.vertex_buffers[1], 0, cast_slice(&data.1));
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.init.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.init
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        {
            let color_attach = ws::create_color_attachment(&view);
            let msaa_attach = ws::create_msaa_color_attachment(&view, &self.msaa_texture_view);
            let color_attachment = if self.init.sample_count == 1 {
                color_attach
            } else {
                msaa_attach
            };
            let depth_attachment = ws::create_depth_stencil_attachment(&self.depth_texture_view);

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(color_attachment)],
                depth_stencil_attachment: Some(depth_attachment),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            let plot_type = if self.plot_type == 1 {
                "shape_only"
            } else if self.plot_type == 2 {
                "wireframe_only"
            } else {
                "both"
            };

            if plot_type == "shape_only" || plot_type == "both" {
                render_pass.set_pipeline(&self.pipelines[0]);
                render_pass.set_vertex_buffer(0, self.vertex_buffers[0].slice(..));
                render_pass
                    .set_index_buffer(self.index_buffers[0].slice(..), wgpu::IndexFormat::Uint16);
                render_pass.set_bind_group(0, &self.uniform_bind_groups[0], &[]);
                render_pass.set_bind_group(1, &self.uniform_bind_groups[1], &[]);
                render_pass.draw_indexed(0..self.indices_lens[0], 0, 0..self.objects_count);
            }

            if plot_type == "wireframe_only" || plot_type == "both" {
                render_pass.set_pipeline(&self.pipelines[1]);
                render_pass.set_vertex_buffer(0, self.vertex_buffers[1].slice(..));
                render_pass
                    .set_index_buffer(self.index_buffers[1].slice(..), wgpu::IndexFormat::Uint16);
                render_pass.set_bind_group(0, &self.uniform_bind_groups[2], &[]);
                render_pass.set_bind_group(1, &self.uniform_bind_groups[3], &[]);
                render_pass.draw_indexed(0..self.indices_lens[1], 0, 0..self.objects_count);
            }

			self.fps_counter.print_fps(5);
        }

        self.init.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
