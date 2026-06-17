use rapier3d::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn run() {
    console_log::init_with_level(log::Level::Info).unwrap_throw();
    log::info!("starting rust");

    let window = wgpu::web_sys::window().unwrap();
    let document = window.document().unwrap();
    let text = document.get_element_by_id("foo").unwrap();

    let mut ticker = Ticker::new(1.0);
    let callback = Rc::new(RefCell::new(None));
    let callback_clone = callback.clone();
    let mut state = State::new();
    *callback_clone.borrow_mut() = Some(Closure::new(move || {
        let steps = ticker.tick(30.0 / 60.0);
        for _ in 0..steps {
            log::info!("step");
            state.step();
        }

        let s = format!("Ball altitude: {}", state.ball_y()).to_string();
        text.set_text_content(Some(&s));
        request_animation_frame(callback.borrow().as_ref().unwrap());
    }));
    request_animation_frame(callback_clone.borrow().as_ref().unwrap());
}

struct State {
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
    fn new() -> State {
        let mut rigid_body_set = RigidBodySet::new();
        let mut collider_set = ColliderSet::new();

        /* Create the ground. */
        let collider = ColliderBuilder::cuboid(100.0, 0.1, 0.1).build();
        collider_set.insert(collider);

        /* Create the bouncing ball. */
        let rigid_body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(0.0, 10.0, 0.0))
            .build();
        let collider = ColliderBuilder::ball(0.5).restitution(0.7).build();
        let ball_body_handle = rigid_body_set.insert(rigid_body);
        collider_set.insert_with_parent(collider, ball_body_handle, &mut rigid_body_set);

        /* Create other structures necessary for the simulation. */
        let gravity = Vector::new(0.0, -9.81, 0.0);
        let integration_parameters = IntegrationParameters::default();
        let physics_pipeline = PhysicsPipeline::new();
        let island_manager = IslandManager::new();
        let broad_phase = DefaultBroadPhase::new();
        let narrow_phase = NarrowPhase::new();
        let impulse_joint_set = ImpulseJointSet::new();
        let multibody_joint_set = MultibodyJointSet::new();
        let ccd_solver = CCDSolver::new();
        return State {
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
        };
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
    }
    fn ball_y(&mut self) -> f32 {
        for (_, b) in self.bodies.iter() {
            return b.translation().y;
        }
        0.0
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
        self.accumulator += frame_dt.clamp(0.0, 0.25); // avoid spiral of death
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
