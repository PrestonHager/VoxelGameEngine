//! Rapier3D integration: broadphase + bodies (voxel collision hooks can plug in later).

use rapier3d::prelude::*;

/// Owns Rapier pipeline state for one simulation configuration.
pub struct PhysicsWorld {
    pub pipeline: PhysicsPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: BroadPhaseMultiSap,
    pub narrow_phase: NarrowPhase,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub impulse_joints: ImpulseJointSet,
    pub multibody_joints: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    pub integration_parameters: IntegrationParameters,
    pub gravity: Vector<f32>,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self {
            pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhaseMultiSap::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            integration_parameters: IntegrationParameters::default(),
            gravity: vector![0.0, -9.81, 0.0],
        }
    }
}

impl PhysicsWorld {
    pub fn step(&mut self) {
        self.pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );
    }

    /// Static ground + dynamic cube demo (for tests / harness).
    pub fn demo_stack() -> Self {
        let mut w = Self::default();
        let ground = ColliderBuilder::cuboid(10.0, 0.1, 10.0).build();
        let ground_body = RigidBodyBuilder::fixed().build();
        let gh = w.bodies.insert(ground_body);
        w.colliders.insert_with_parent(ground, gh, &mut w.bodies);

        let body = RigidBodyBuilder::dynamic()
            .translation(vector![0.0, 2.0, 0.0])
            .build();
        let coll = ColliderBuilder::cuboid(0.5, 0.5, 0.5).build();
        let h = w.bodies.insert(body);
        w.colliders.insert_with_parent(coll, h, &mut w.bodies);
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_falls() {
        let mut w = PhysicsWorld::demo_stack();
        let handle = w
            .bodies
            .iter()
            .find(|(_, b)| b.is_dynamic())
            .expect("dynamic body")
            .0;
        let y0 = w.bodies[handle].translation().y;
        for _ in 0..60 {
            w.step();
        }
        let y1 = w.bodies[handle].translation().y;
        assert!(y1 < y0, "body should fall: {y0} -> {y1}");
    }
}
