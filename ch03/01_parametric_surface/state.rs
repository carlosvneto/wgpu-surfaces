use bytemuck::cast_slice;
use cgmath::{Matrix, Matrix4, SquareMatrix};
use wgpu::util::DeviceExt;
use winit::{
    event::ElementState, event::KeyEvent, event::WindowEvent, keyboard::Key, keyboard::NamedKey,
    window::Window,
};
use rand::Rng;
use rand::rngs::ThreadRng;

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
    update_buffers: bool,
    recreate_buffers: bool,
    rotation_speed: f32,
    rng: ThreadRng,
    t0: std::time::Instant,
    random_shape_change: u32,

    parametric_surface: sd::IParametricSurface,
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
            .create_shader_module(wgpu::include_wgsl!("../../ch02/01_simple_surface/shader_vert.wgsl"));
        let fs_shader = init
            .device
            .create_shader_module(wgpu::include_wgsl!("../../ch02/common/directional_frag.wgsl"));

        // uniform data
        let camera_position = (2.0, 2.0, 4.0).into();
        let look_direction = (0.0, 0.0, 0.0).into();
        let up_direction = cgmath::Vector3::unit_y();
        let light_direction = [-0.5f32, -0.5, -0.5];

        let (view_mat, project_mat, _) = ws::create_vp_mat(
            camera_position,
            look_direction,
            up_direction,
            init.config.width as f32 / init.config.height as f32,
        );

        // create vertex uniform buffers

        // model_mat and vp_mat will be stored in vertex_uniform_buffer inside the update function
        let vert_uniform_buffer = init.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Uniform Buffer"),
            size: 192,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
        let (vert_bind_group_layout, vert_bind_group) = ws::create_bind_group(
            &init.device,
            vec![wgpu::ShaderStages::VERTEX],
            &[vert_uniform_buffer.as_entire_binding()],
        );
        let (vert_bind_group_layout2, vert_bind_group2) = ws::create_bind_group(
            &init.device,
            vec![wgpu::ShaderStages::VERTEX],
            &[vert_uniform_buffer.as_entire_binding()],
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

        let mut ps = sd::IParametricSurface {
            scale: 4.5,
            surface_type: 0,
            colormap_name: colormap_name.to_string(),
            wireframe_color: wireframe_color.to_string(),
            ..Default::default()
        };
        let data = create_vertices(ps.new());

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
                vert_uniform_buffer,
                light_uniform_buffer,
                material_uniform_buffer,
            ],
            view_mat,
            project_mat,
            msaa_texture_view,
            depth_texture_view,
            indices_lens: vec![data.2.len() as u32, data.3.len() as u32],
            plot_type: 1,
            update_buffers: false,
            recreate_buffers: false,
            rotation_speed: 1.0,
            rng: rand::rng(),
            t0: std::time::Instant::now(),
            random_shape_change: 1,

            parametric_surface: ps,
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
                    self.parametric_surface.surface_type =
                        (self.parametric_surface.surface_type + 1) % 23;
                    self.update_buffers = true;
                    return true;
                }
                Key::Named(NamedKey::Shift) => {
                    self.parametric_surface.colormap_direction =
                        (self.parametric_surface.colormap_direction + 1) % 3;
                    self.update_buffers = true;
                    return true;
                }
                Key::Named(NamedKey::Alt) => {
                    self.random_shape_change = (self.random_shape_change + 1) % 2;
                    return true;
                }
                Key::Character("q") => {
                    self.parametric_surface.u_resolution += 1;
                    self.recreate_buffers = true;
                    return true;
                }
                Key::Character("a") => {
                    self.parametric_surface.u_resolution -= 1;
                    if self.parametric_surface.u_resolution < 8 {
                        self.parametric_surface.u_resolution = 8;
                    }
                    self.recreate_buffers = true;
                    return true;
                }
                Key::Character("w") => {
                    self.parametric_surface.v_resolution += 1;
                    self.recreate_buffers = true;
                    return true;
                }
                Key::Character("s") => {
                    self.parametric_surface.v_resolution -= 1;
                    if self.parametric_surface.v_resolution < 8 {
                        self.parametric_surface.v_resolution = 8;
                    }
                    self.recreate_buffers = true;
                    return true;
                }
                Key::Character("e") => {
                    self.rotation_speed += 0.1;
                    return true;
                }
                Key::Character("d") => {
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
        let dt1 = self.rotation_speed * dt.as_secs_f32();

        let model_mat = ws::create_model_mat(
            [0.0, 0.0, 0.0],
            [dt1.sin(), dt1.cos(), 0.0],
            [1.0, 1.0, 1.0],
        );
        let view_project_mat = self.project_mat * self.view_mat;

        let normal_mat = (model_mat.invert().unwrap()).transpose();

        let model_ref: &[f32; 16] = model_mat.as_ref();
        let view_projection_ref: &[f32; 16] = view_project_mat.as_ref();
        let normal_ref: &[f32; 16] = normal_mat.as_ref();

        self.init.queue.write_buffer(
            &self.uniform_buffers[0],
            0,
            bytemuck::cast_slice(view_projection_ref),
        );
        self.init.queue.write_buffer(
            &self.uniform_buffers[0],
            64,
            bytemuck::cast_slice(model_ref),
        );
        self.init.queue.write_buffer(
            &self.uniform_buffers[0],
            128,
            bytemuck::cast_slice(normal_ref),
        );

        // recreate vertex and index buffers
        if self.recreate_buffers {
            let data = create_vertices(self.parametric_surface.new());
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

        // update vertex buffer for every 5 seconds
        let elapsed = self.t0.elapsed();
        if elapsed >= std::time::Duration::from_secs(5) && self.random_shape_change == 1 {
            self.parametric_surface.surface_type = self.rng.random_range(0..=22) as u32;
            let data = create_vertices(self.parametric_surface.new());
            self.init
                .queue
                .write_buffer(&self.vertex_buffers[0], 0, cast_slice(&data.0));
            self.init
                .queue
                .write_buffer(&self.vertex_buffers[1], 0, cast_slice(&data.1));
            self.t0 = std::time::Instant::now();

            println!(
                "key = {:?}, value = {:?}",
                self.parametric_surface.surface_type,
                self.parametric_surface.surface_type_map[&self.parametric_surface.surface_type]
            );
        }

        // update vertex buffer when data changed
        if self.update_buffers {
            let data = create_vertices(self.parametric_surface.new());
            self.init
                .queue
                .write_buffer(&self.vertex_buffers[0], 0, cast_slice(&data.0));
            self.init
                .queue
                .write_buffer(&self.vertex_buffers[1], 0, cast_slice(&data.1));
            self.update_buffers = false;
        }
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
                render_pass.draw_indexed(0..self.indices_lens[0], 0, 0..1);
            }

            if plot_type == "wireframe_only" || plot_type == "both" {
                render_pass.set_pipeline(&self.pipelines[1]);
                render_pass.set_vertex_buffer(0, self.vertex_buffers[1].slice(..));
                render_pass
                    .set_index_buffer(self.index_buffers[1].slice(..), wgpu::IndexFormat::Uint16);
                render_pass.set_bind_group(0, &self.uniform_bind_groups[2], &[]);
                render_pass.set_bind_group(1, &self.uniform_bind_groups[3], &[]);
                render_pass.draw_indexed(0..self.indices_lens[1], 0, 0..1);
            }
            
            self.fps_counter.print_fps(5);
        }

        self.init.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
