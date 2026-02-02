use glam::Vec3;
use rapier3d::control::{EffectiveCharacterMovement, KinematicCharacterController};
use rapier3d::prelude::*;

use super::PhysicsSnapshot;

pub struct PhysicsWorld {
    pipeline: PhysicsPipeline,
    integration_parameters: IntegrationParameters,
    islands: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    gravity: Vector,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorld {
    const TICK_RATE: Real = 1.0 / 60.0;

    pub fn new() -> Self {
        let mut integration_parameters = IntegrationParameters::default();
        integration_parameters.dt = Self::TICK_RATE;
        integration_parameters.min_ccd_dt = Self::TICK_RATE / 100.0;

        Self {
            pipeline: PhysicsPipeline::new(),
            integration_parameters,
            islands: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            gravity: Vector::new(0.0, -9.81, 0.0),
        }
    }

    pub fn step(&mut self) {
        self.pipeline.step(
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
            &(),
            &(),
        );
    }

    pub fn snapshot(&self) -> PhysicsSnapshot {
        PhysicsSnapshot {
            bodies: self.bodies.clone(),
            colliders: self.colliders.clone(),
            islands: self.islands.clone(),
            impulse_joints: self.impulse_joints.clone(),
            multibody_joints: self.multibody_joints.clone(),
        }
    }

    pub fn restore(&mut self, snapshot: &PhysicsSnapshot) {
        self.bodies = snapshot.bodies.clone();
        self.colliders = snapshot.colliders.clone();
        self.islands = snapshot.islands.clone();
        self.impulse_joints = snapshot.impulse_joints.clone();
        self.multibody_joints = snapshot.multibody_joints.clone();

        self.broad_phase = DefaultBroadPhase::new();
        self.narrow_phase = NarrowPhase::new();
    }

    pub fn add_player(&mut self, position: Vec3, radius: Real, height: Real) -> RigidBodyHandle {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(Vector::new(position.x, position.y, position.z))
            .lock_rotations()
            .build();

        let handle = self.bodies.insert(body);

        let half_height = height / 2.0;
        let collider = ColliderBuilder::cylinder(half_height, radius)
            .friction(0.0)
            .build();

        self.colliders
            .insert_with_parent(collider, handle, &mut self.bodies);

        handle
    }

    pub fn add_kinematic(&mut self, position: Vec3) -> RigidBodyHandle {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(Vector::new(position.x, position.y, position.z))
            .build();
        self.bodies.insert(body)
    }

    pub fn add_kinematic_box(&mut self, position: Vec3, half_extents: Vec3) -> RigidBodyHandle {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(Vector::new(position.x, position.y, position.z))
            .build();
        let handle = self.bodies.insert(body);

        let collider =
            ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z).build();
        self.colliders
            .insert_with_parent(collider, handle, &mut self.bodies);

        handle
    }

    pub fn add_static_box(&mut self, position: Vec3, half_extents: Vec3) -> ColliderHandle {
        let collider = ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            .translation(Vector::new(position.x, position.y, position.z))
            .build();
        self.colliders.insert(collider)
    }

    pub fn add_ground(&mut self, y: Real, half_size: Real) -> ColliderHandle {
        let collider = ColliderBuilder::cuboid(half_size, 0.1, half_size)
            .translation(Vector::new(0.0, y, 0.0))
            .build();
        self.colliders.insert(collider)
    }

    pub fn add_dynamic_box(
        &mut self,
        position: Vec3,
        half_extents: Vec3,
        mass: Real,
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(position.x, position.y, position.z))
            .ccd_enabled(true)
            .build();

        let handle = self.bodies.insert(body);

        let collider = ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            .mass(mass)
            .friction(0.5)
            .restitution(0.3)
            .build();

        self.colliders
            .insert_with_parent(collider, handle, &mut self.bodies);

        handle
    }

    pub fn add_dynamic_sphere(
        &mut self,
        position: Vec3,
        radius: f32,
        mass: f32,
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::dynamic()
            .translation(Vector::new(position.x, position.y, position.z))
            .ccd_enabled(true)
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::ball(radius)
            .mass(mass)
            .friction(0.5)
            .restitution(0.3)
            .build();
        self.colliders
            .insert_with_parent(collider, handle, &mut self.bodies);
        handle
    }

    pub fn add_kinematic_sphere(&mut self, position: Vec3, radius: f32) -> RigidBodyHandle {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(Vector::new(position.x, position.y, position.z))
            .build();
        let handle = self.bodies.insert(body);
        let collider = ColliderBuilder::ball(radius).build();
        self.colliders
            .insert_with_parent(collider, handle, &mut self.bodies);
        handle
    }

    pub fn set_next_kinematic_pose(
        &mut self,
        handle: RigidBodyHandle,
        position: Vec3,
        rotation: glam::Quat,
    ) {
        if let Some(body) = self.bodies.get_mut(handle) {
            let rot =
                Rotation::from_xyzw(rotation.x, rotation.y, rotation.z, rotation.w).normalize();
            let new_pose = Pose::from_parts(Vector::new(position.x, position.y, position.z), rot);
            body.set_next_kinematic_position(new_pose);
        }
    }

    pub fn remove_body(&mut self, handle: RigidBodyHandle) {
        self.bodies.remove(
            handle,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    pub fn body(&self, handle: RigidBodyHandle) -> Option<&RigidBody> {
        self.bodies.get(handle)
    }

    pub fn body_mut(&mut self, handle: RigidBodyHandle) -> Option<&mut RigidBody> {
        self.bodies.get_mut(handle)
    }

    pub fn set_body_position(&mut self, handle: RigidBodyHandle, position: Vec3) {
        if let Some(body) = self.bodies.get_mut(handle) {
            let current_rot = *body.rotation();
            let new_pose =
                Pose::from_parts(Vector::new(position.x, position.y, position.z), current_rot);
            body.set_position(new_pose, true);
        }
    }

    pub fn set_body_pose(&mut self, handle: RigidBodyHandle, position: Vec3, rotation: glam::Quat) {
        if let Some(body) = self.bodies.get_mut(handle) {
            let rot =
                Rotation::from_xyzw(rotation.x, rotation.y, rotation.z, rotation.w).normalize();
            let new_pose = Pose::from_parts(Vector::new(position.x, position.y, position.z), rot);
            body.set_position(new_pose, true);
        }
    }

    pub fn set_body_velocity(&mut self, handle: RigidBodyHandle, velocity: Vec3) {
        if let Some(body) = self.bodies.get_mut(handle) {
            body.set_linvel(Vector::new(velocity.x, velocity.y, velocity.z), true);
        }
    }

    pub fn apply_impulse(&mut self, handle: RigidBodyHandle, impulse: Vec3) {
        if let Some(body) = self.bodies.get_mut(handle) {
            body.apply_impulse(Vector::new(impulse.x, impulse.y, impulse.z), true);
        }
    }

    pub fn body_position(&self, handle: RigidBodyHandle) -> Option<Vec3> {
        self.bodies.get(handle).map(|b| {
            let t = b.translation();
            Vec3::new(t.x, t.y, t.z)
        })
    }

    pub fn body_velocity(&self, handle: RigidBodyHandle) -> Option<Vec3> {
        self.bodies.get(handle).map(|b| {
            let v = b.linvel();
            Vec3::new(v.x, v.y, v.z)
        })
    }

    pub fn set_player_height(&mut self, handle: RigidBodyHandle, height: Real, radius: Real) {
        let Some(body) = self.bodies.get(handle) else {
            return;
        };

        let collider_handles: Vec<_> = body.colliders().to_vec();
        for collider_handle in collider_handles {
            if let Some(collider) = self.colliders.get_mut(collider_handle) {
                let half_height = height / 2.0;
                collider.set_shape(rapier3d::geometry::SharedShape::cylinder(
                    half_height,
                    radius,
                ));
            }
        }
    }

    pub fn move_character(
        &self,
        controller: &KinematicCharacterController,
        handle: RigidBodyHandle,
        shape: &SharedShape,
        position: Pose,
        desired_translation: Vector,
        _dt: f32,
    ) -> EffectiveCharacterMovement {
        let filter = QueryFilter::default().exclude_rigid_body(handle);
        let query_pipeline = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        );

        controller.move_shape(
            self.integration_parameters.dt,
            &query_pipeline,
            shape.as_ref(),
            &position,
            desired_translation,
            |_collision| {},
        )
    }

    fn query_pipeline(&self) -> QueryPipeline<'_> {
        self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default(),
        )
    }

    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: Real,
    ) -> Option<(Vec3, Real)> {
        let query = self.query_pipeline();
        let ray = Ray::new(
            Vector::new(origin.x, origin.y, origin.z),
            Vector::new(direction.x, direction.y, direction.z),
        );

        query.cast_ray(&ray, max_distance, true).map(|(_, toi)| {
            let hit_point = origin + direction * toi;
            (hit_point, toi)
        })
    }

    pub fn is_grounded(&self, handle: RigidBodyHandle, threshold: Real) -> bool {
        let Some(body) = self.bodies.get(handle) else {
            return false;
        };

        let query = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            QueryFilter::default().exclude_rigid_body(handle),
        );

        let pos = body.translation();
        let ray = Ray::new(
            Vector::new(pos.x, pos.y, pos.z),
            Vector::new(0.0, -1.0, 0.0),
        );

        query.cast_ray(&ray, threshold, true).is_some()
    }
}
