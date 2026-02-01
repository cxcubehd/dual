use glam::Vec3;

use crate::physics::PhysicsWorld;
use crate::snapshot::{EntityHandle, EntityType, World};

use super::{MapObject, MapObjectKind};

pub struct TestingGround {
    objects: Vec<MapObject>,
}

impl Default for TestingGround {
    fn default() -> Self {
        Self::new()
    }
}

impl TestingGround {
    const GROUND_SIZE: f32 = 100.0;
    const GROUND_Y: f32 = 0.0;

    pub fn new() -> Self {
        let mut objects = Vec::new();

        objects.push(MapObject::ground(
            Vec3::new(0.0, Self::GROUND_Y, 0.0),
            Self::GROUND_SIZE,
        ));

        Self::add_platform_obstacles(&mut objects);
        Self::add_stair_platforms(&mut objects);
        Self::add_dynamic_props(&mut objects);

        Self { objects }
    }

    fn add_platform_obstacles(objects: &mut Vec<MapObject>) {
        objects.push(MapObject::static_box(
            Vec3::new(5.0, 0.25, 0.0),
            Vec3::new(1.0, 0.25, 1.0),
        ));

        objects.push(MapObject::static_box(
            Vec3::new(8.0, 0.5, 0.0),
            Vec3::new(1.0, 0.5, 1.0),
        ));

        objects.push(MapObject::static_box(
            Vec3::new(11.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 1.0),
        ));

        objects.push(MapObject::static_box(
            Vec3::new(14.0, 1.5, 0.0),
            Vec3::new(1.5, 1.5, 1.5),
        ));

        objects.push(MapObject::static_box(
            Vec3::new(18.0, 2.0, 0.0),
            Vec3::new(2.0, 2.0, 2.0),
        ));
    }

    fn add_stair_platforms(objects: &mut Vec<MapObject>) {
        let stair_start = Vec3::new(-5.0, 0.0, 5.0);
        let step_height = 0.3;
        let step_depth = 0.4;
        let step_width = 2.0;

        for i in 0..10 {
            let y = step_height * (i as f32 + 0.5);
            let z = stair_start.z + step_depth * i as f32;
            objects.push(MapObject::static_box(
                Vec3::new(stair_start.x, y, z),
                Vec3::new(step_width, step_height * 0.5, step_depth * 0.5),
            ));
        }
    }

    fn add_dynamic_props(objects: &mut Vec<MapObject>) {
        objects.push(MapObject::dynamic_box(
            Vec3::new(3.0, 1.0, 3.0),
            Vec3::new(0.3, 0.3, 0.3),
            5.0,
        ));

        objects.push(MapObject::dynamic_box(
            Vec3::new(3.5, 1.0, 3.0),
            Vec3::new(0.2, 0.2, 0.2),
            2.0,
        ));

        objects.push(MapObject::dynamic_box(
            Vec3::new(4.0, 1.0, 3.0),
            Vec3::new(0.4, 0.4, 0.4),
            10.0,
        ));

        for i in 0..5 {
            objects.push(MapObject::dynamic_box(
                Vec3::new(-3.0 + i as f32 * 0.5, 0.5 + i as f32 * 0.5, -5.0),
                Vec3::new(0.25, 0.25, 0.25),
                3.0,
            ));
        }

        objects.push(MapObject::dynamic_box(
            Vec3::new(0.0, 2.0, -8.0),
            Vec3::new(0.5, 0.5, 0.5),
            20.0,
        ));
    }

    pub fn objects(&self) -> &[MapObject] {
        &self.objects
    }

    pub fn spawn(&mut self, world: &mut World, physics: &mut PhysicsWorld) {
        for object in &mut self.objects {
            match object.kind {
                MapObjectKind::Ground => {
                    physics.add_ground(object.position.y, object.half_extents.x);
                }
                MapObjectKind::StaticBox => {
                    physics.add_static_box(object.position, object.half_extents);
                }
                MapObjectKind::DynamicBox => {
                    let handle = world.spawn(EntityType::DynamicProp);
                    object.entity_id = Some(handle.id());

                    if let Some(entity) = world.get_mut(handle) {
                        entity.position = object.position;

                        let physics_handle = physics.add_dynamic_box(
                            object.position,
                            object.half_extents,
                            object.mass.unwrap_or(1.0),
                        );
                        entity.physics_handle = Some(physics_handle);
                    }
                }
            }
        }
    }
    
    /// Spawn only physics colliders without creating world entities.
    /// Used for client-side prediction where we only need collision geometry.
    pub fn spawn_physics_only(physics: &mut PhysicsWorld) {
        let ground = Self::new();
        
        for object in &ground.objects {
            match object.kind {
                MapObjectKind::Ground => {
                    physics.add_ground(object.position.y, object.half_extents.x);
                }
                MapObjectKind::StaticBox => {
                    physics.add_static_box(object.position, object.half_extents);
                }
                MapObjectKind::DynamicBox => {
                    // For prediction, we treat dynamic props as static colliders
                    // since we can't simulate their full physics state locally
                    physics.add_static_box(object.position, object.half_extents);
                }
            }
        }
    }

    pub fn dynamic_entity_handles(&self) -> Vec<EntityHandle> {
        self.objects
            .iter()
            .filter_map(|obj| obj.entity_id.map(EntityHandle))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testing_ground_spawns_objects() {
        let mut ground = TestingGround::new();
        let mut world = World::new();
        let mut physics = PhysicsWorld::new();

        ground.spawn(&mut world, &mut physics);

        assert!(world.entity_count() > 0);
        assert!(!ground.dynamic_entity_handles().is_empty());
    }
}
