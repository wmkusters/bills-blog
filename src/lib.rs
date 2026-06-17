use rapier2d::prelude::*;
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

    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    /* Create the ground. */
    let collider = ColliderBuilder::cuboid(100.0, 0.1).build();
    collider_set.insert(collider);

    /* Create the bouncing ball. */
    let rigid_body = RigidBodyBuilder::dynamic()
        .translation(Vector::new(0.0, 10.0))
        .build();
    let collider = ColliderBuilder::ball(0.5).restitution(0.7).build();
    let ball_body_handle = rigid_body_set.insert(rigid_body);
    collider_set.insert_with_parent(collider, ball_body_handle, &mut rigid_body_set);

    /* Create other structures necessary for the simulation. */
    let gravity = Vector::new(0.0, -9.81);
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = DefaultBroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut impulse_joint_set = ImpulseJointSet::new();
    let mut multibody_joint_set = MultibodyJointSet::new();
    let mut ccd_solver = CCDSolver::new();
    let physics_hooks = ();
    let event_handler = ();

    let mut ticker = Ticker::new(1.0);
    let callback = Rc::new(RefCell::new(None));
    let callback_clone = callback.clone();
    *callback_clone.borrow_mut() = Some(Closure::new(move || {
        let steps = ticker.tick(1.0 / 60.0);
        for _ in 0..steps {
            log::info!("step");
            physics_pipeline.step(
                gravity,
                &integration_parameters,
                &mut island_manager,
                &mut broad_phase,
                &mut narrow_phase,
                &mut rigid_body_set,
                &mut collider_set,
                &mut impulse_joint_set,
                &mut multibody_joint_set,
                &mut ccd_solver,
                &physics_hooks,
                &event_handler,
            );
        }
        let ball_body = &rigid_body_set[ball_body_handle];
        let s = format!("Ball altitude: {}", ball_body.translation().y).to_string();
        text.set_text_content(Some(&s));
        request_animation_frame(callback.borrow().as_ref().unwrap());
    }));
    request_animation_frame(callback_clone.borrow().as_ref().unwrap());
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
