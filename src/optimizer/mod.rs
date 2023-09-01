use tracing::span;

use crate::planner::logical_plan::LogicalPlan;

use self::{
    heuristic::HepOptimizer, physical_optimizer::PhysicalOptimizer, physical_plan::PhysicalPlan,
};

pub mod heuristic;
pub mod physical_optimizer;
pub mod physical_plan;
pub mod rule;

pub struct Optimizer {
    hep_optimizer: HepOptimizer,
    physical_optimizer: PhysicalOptimizer,
}
impl Optimizer {
    pub fn new(logical_plan: LogicalPlan) -> Self {
        Self {
            hep_optimizer: HepOptimizer::default_optimizer(logical_plan),
            physical_optimizer: PhysicalOptimizer {},
        }
    }

    pub fn find_best(&mut self) -> PhysicalPlan {
        let _find_best_span = span!(tracing::Level::INFO, "optimizer.find_best").entered();
        // optimize logical plan
        let optimized_logical_plan = self.hep_optimizer.find_best();

        // optimize physical plan
        self.physical_optimizer.find_best(optimized_logical_plan)
    }
}
