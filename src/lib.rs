use rapier3d::prelude::*;
use std::cell::RefCell;
use std::iter;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

mod camera;
mod model;
mod resources;
mod texture;

use camera::{Camera, CameraUniform};
use model::{DrawModel, Vertex};

struct Instance {
    position: cgmath::Vector3<f32>,
    rotation: cgmath::Quaternion<f32>,
    scale: f32,
}

// NEW!
impl Instance {
    fn to_raw(&self) -> InstanceRaw {
        InstanceRaw {
            model: (cgmath::Matrix4::from_translation(self.position)
                * cgmath::Matrix4::from(self.rotation)
                * cgmath::Matrix4::from_scale(self.scale))
            .into(),
        }
    }
}

// NEW!
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
}

impl InstanceRaw {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            // We need to switch from using a step mode of Vertex to Instance
            // This means that our shaders will only change to use the next
            // instance when the shader starts processing a new instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    // While our vertex shader only uses locations 0, and 1 now, in later tutorials we'll
                    // be using 2, 3, and 4, for Vertex. We'll start at slot 5 not conflict with them later
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // A mat4 takes up 4 vertex slots as it is technically 4 vec4s. We need to define a slot
                // for each vec4. We don't have to do this in code though.
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

async fn arun() -> anyhow::Result<()> {
    let mut ticker = Ticker::new(1.0);
    let callback = Rc::new(RefCell::new(None));
    let callback_clone = callback.clone();
    let document = web_sys::window().unwrap().document().unwrap();
    let text = document.get_element_by_id("foo").unwrap();

    let mut state = State::new().await.unwrap();
    state.resize();
    *callback_clone.borrow_mut() = Some(Closure::new(move || {
        state.resize();
        state.render().unwrap();
        let steps = ticker.tick(240.0 / 60.0);
        log::info!("running {} steps", steps);
        for _ in 0..steps {
            state.step();
        }

        let s = format!(
            "Ball x: {}, ball y: {}, ball z: {}",
            state.ball_x(),
            state.ball_y(),
            state.ball_z()
        )
        .to_string();
        text.set_text_content(Some(&s));
        request_animation_frame(callback.borrow().as_ref().unwrap());
    }));
    request_animation_frame(callback_clone.borrow().as_ref().unwrap());
    Ok(())
}

#[wasm_bindgen(start)]
pub fn run() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).unwrap_throw();
    log::info!("starting rust");

    wasm_bindgen_futures::spawn_local(async {
        arun().await.unwrap_throw();
    });
}

struct State {
    // graphics
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    is_surface_configured: bool,
    render_pipeline: wgpu::RenderPipeline,
    obj_model: model::Model,
    camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,
    depth_view: wgpu::TextureView,
    // physics
    physics_pipeline: PhysicsPipeline,
    gravity: Vector,
    integration_parameters: IntegrationParameters,
    islands: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    //hooks: OptionBox<dyn PhysicsHooks>,
    //events: Box<dyn EventHandler>,
}

impl State {
    async fn new() -> anyhow::Result<State> {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id("canvas")
            .unwrap()
            .dyn_into::<HtmlCanvasElement>()
            .unwrap();
        let window = web_sys::window().unwrap();
        let dpr = window.device_pixel_ratio();
        let rect = canvas.get_bounding_client_rect();
        let width = (rect.width() * dpr) as u32;
        let height = (rect.height() * dpr) as u32;

        canvas.set_width(width);
        canvas.set_height(height);

        let GraphicsState {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            obj_model,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            depth_view,
        } = setup_graphics(&canvas).await?;

        /* physics */
        let mut rigid_body_set = RigidBodySet::new();
        let mut collider_set = ColliderSet::new();

        /* Create the ground. */
        let collider = ColliderBuilder::cuboid(10.0, 10.0, 10.0)
            .translation(Vector::new(0.0, -10.0, 0.0))
            .friction(0.1)
            .restitution(0.0)
            .build();
        collider_set.insert(collider);
        let collider = ColliderBuilder::cuboid(10.0, 10.0, 10.0)
            .translation(Vector::new(0.0, 10.0, 20.0))
            .friction(0.1)
            .restitution(0.0)
            .build();
        collider_set.insert(collider);
        let collider = ColliderBuilder::cuboid(10.0, 10.0, 10.0)
            .translation(Vector::new(-20.0, 10.0, 0.0))
            .friction(0.1)
            .restitution(0.0)
            .build();
        collider_set.insert(collider);
        let collider = ColliderBuilder::cuboid(10.0, 10.0, 10.0)
            .translation(Vector::new(0.0, 10.0, -20.0))
            .friction(0.1)
            .restitution(0.0)
            .build();
        collider_set.insert(collider);
        let collider = ColliderBuilder::cuboid(10.0, 10.0, 10.0)
            .translation(Vector::new(0.0, 20.0, 0.0))
            .friction(0.1)
            .restitution(0.0)
            .build();
        collider_set.insert(collider);

        let mut instances = vec![];
        //for (_, c) in collider_set.iter() {
        //    instances.push(Instance {
        //        position: cgmath::vec3(c.translation().x, c.translation().y, c.translation().z),
        //        rotation: cgmath::Quaternion::new(
        //            c.rotation().w,
        //            c.rotation().x,
        //            c.rotation().y,
        //            c.rotation().z,
        //        ),
        //        scale: c.shape().as_cuboid().unwrap().half_extents.x,
        //    });
        //}
        instances.push(Instance {
            position: cgmath::vec3(0.0, 10.0, 0.0),
            rotation: cgmath::Quaternion::from_sv(1.0, cgmath::vec3(0.0, 0.0, 0.0)),
            scale: 0.5,
        });
        let mut rigid_body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(0.0, 0.50, 0.0))
            .rotation(Vector::new(
                std::f32::consts::PI / 4.0,
                std::f32::consts::PI / 4.0,
                0.0,
            ))
            .linvel(Vector::new(0.0, 0.0, -7.5))
            .build();
        rigid_body.set_enabled_translations(false, true, true, true);
        let collider = ColliderBuilder::ball(0.5).restitution(0.0).build();
        let ball_body_handle = rigid_body_set.insert(rigid_body);
        collider_set.insert_with_parent(collider, ball_body_handle, &mut rigid_body_set);

        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        /* Create other structures necessary for the simulation. */
        let gravity = Vector::new(0.0, -9.81, -0.0);
        let integration_parameters = IntegrationParameters::default();
        let physics_pipeline = PhysicsPipeline::new();
        let island_manager = IslandManager::new();
        let broad_phase = DefaultBroadPhase::new();
        let narrow_phase = NarrowPhase::new();
        let impulse_joint_set = ImpulseJointSet::new();
        let multibody_joint_set = MultibodyJointSet::new();
        let ccd_solver = CCDSolver::new();
        return Ok(State {
            canvas,
            surface,
            device,
            queue,
            config,
            is_surface_configured: false,
            render_pipeline,
            obj_model,
            camera,
            camera_bind_group,
            camera_uniform,
            camera_buffer,
            instances,
            instance_buffer,
            depth_view,
            physics_pipeline,
            gravity,
            integration_parameters,
            islands: island_manager,
            broad_phase,
            narrow_phase,
            bodies: rigid_body_set,
            colliders: collider_set,
            impulse_joints: impulse_joint_set,
            multibody_joints: multibody_joint_set,
            ccd_solver,
        });
    }

    fn step(&mut self) {
        let hooks = ();
        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &hooks,
            &hooks,
        );
        let (_, bod) = self.bodies.iter().next().unwrap();

        if let Some(b) = self.instances.last_mut() {
            b.position = cgmath::vec3(
                bod.translation().x,
                bod.translation().y,
                bod.translation().z,
            );
            b.rotation = cgmath::Quaternion::new(
                bod.rotation().w,
                bod.rotation().x,
                bod.rotation().y,
                bod.rotation().z,
            );
        }

        let instance_data = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instance_data),
        );
    }

    fn ball_y(&mut self) -> f32 {
        for (_, b) in self.bodies.iter() {
            return b.translation().y;
        }
        0.0
    }

    fn ball_x(&mut self) -> f32 {
        for (_, b) in self.bodies.iter() {
            return b.translation().x;
        }
        0.0
    }

    fn ball_z(&mut self) -> f32 {
        for (_, b) in self.bodies.iter() {
            return b.translation().z;
        }
        0.0
    }

    fn render(&mut self) -> anyhow::Result<()> {
        if !self.is_surface_configured {
            return Ok(());
        }
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                self.surface.configure(&self.device, &self.config);
                surface_texture
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => {
                // Skip this frame
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                // You could recreate the devices and all resources
                // created with it here, but we'll just bail
                // TODO actually bail
                return Ok(());
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.98,
                            g: 0.945,
                            b: 0.78,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.draw_model_instanced(
                &self.obj_model,
                0..self.instances.len() as u32,
                &self.camera_bind_group,
            );
        }

        self.queue.submit(iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn resize(&mut self) {
        let width = self.canvas.width();
        let height = self.canvas.height();
        self.camera.update(width as f32, height as f32);
        self.camera_uniform.update_view_proj(&self.camera);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
        self.depth_view = create_depth_texture(&self.device, &self.config);
        let max_size = self.device.limits().max_texture_dimension_2d;
        if width > 0 && height > 0 {
            self.config.width = width.clamp(1, max_size);
            self.config.height = height.clamp(1, max_size);
            self.surface.configure(&self.device, &self.config);
            self.is_surface_configured = true;
        }
    }
}

fn request_animation_frame(callback: &Closure<dyn FnMut()>) {
    web_sys::window()
        .expect("no global `window` exists")
        .request_animation_frame(callback.as_ref().unchecked_ref())
        .expect("requestAnimationFrame failed");
}

#[wasm_bindgen]
pub struct Ticker {
    fixed_dt: f64,
    accumulator: f64,
    steps_run: u64,
}

#[wasm_bindgen]
impl Ticker {
    /// `fixed_hz` is how many steps per second you want (e.g. 60.0).
    #[wasm_bindgen(constructor)]
    pub fn new(fixed_hz: f64) -> Ticker {
        Ticker {
            fixed_dt: 1.0 / fixed_hz,
            accumulator: 0.0,
            steps_run: 0,
        }
    }

    /// Call once per requestAnimationFrame with the seconds elapsed since
    /// the last call. Returns how many fixed steps should run this frame.
    pub fn tick(&mut self, frame_dt: f64) -> u32 {
        self.accumulator += frame_dt.clamp(0.0, 1.0); // avoid spiral of death
        let mut steps = 0u32;
        while self.accumulator >= self.fixed_dt {
            self.accumulator -= self.fixed_dt;
            self.steps_run += 1;
            steps += 1;
        }
        steps
    }

    pub fn steps_run(&self) -> u64 {
        self.steps_run
    }
}

fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth_texture"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

struct GraphicsState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    obj_model: model::Model,
    camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    // instances: Vec<Instance>,
    // instance_buffer: wgpu::Buffer,
    depth_view: wgpu::TextureView,
}

async fn setup_graphics(canvas: &HtmlCanvasElement) -> anyhow::Result<GraphicsState> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        flags: Default::default(),
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: None,
    });
    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        })
        .await?;

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(surface_caps.formats[0]);
    let max_size = device.limits().max_texture_dimension_2d;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: canvas.width().clamp(1, max_size),
        height: canvas.height().clamp(1, max_size),
        present_mode: surface_caps.present_modes[0],
        alpha_mode: surface_caps.alpha_modes[0],
        desired_maximum_frame_latency: 2,
        view_formats: vec![],
    };
    let depth_view = create_depth_texture(&device, &config);

    let texture_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
            label: Some("texture_bind_group_layout"),
        });

    let camera = Camera::new(config.width as f32, config.height as f32);
    let mut camera_uniform = CameraUniform::new();
    camera_uniform.update_view_proj(&camera);

    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Camera Buffer"),
        contents: bytemuck::cast_slice(&[camera_uniform]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let camera_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            label: Some("camera_bind_group_layout"),
        });

    let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &camera_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
        label: Some("camera_bind_group"),
    });

    let obj_model =
        resources::load_model("ball_solo.obj", &device, &queue, &texture_bind_group_layout)
            .await
            .unwrap();

    let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[
            Some(&texture_bind_group_layout),
            Some(&camera_bind_group_layout),
        ],
        immediate_size: 0,
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[model::ModelVertex::desc(), InstanceRaw::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent::REPLACE,
                    alpha: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::POLYGON_MODE_LINE
            // or Features::POLYGON_MODE_POINT
            polygon_mode: wgpu::PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        // If the pipeline will be used with a multiview render pass, this
        // tells wgpu to render to just specific texture layers.
        multiview_mask: None,
        // Useful for optimizing shader compilation on Android
        cache: None,
    });
    return Ok(GraphicsState {
        surface,
        device,
        queue,
        config,
        render_pipeline,
        obj_model,
        camera,
        camera_bind_group,
        camera_uniform,
        camera_buffer,
        depth_view,
    });
}
