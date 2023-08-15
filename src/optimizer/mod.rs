use std::sync::Arc;

use crate::planner::{
    logical_plan::{self, LogicalPlan},
    operator::LogicalOperator,
};

use self::{
    heuristic::HepOptimizer, physical_optimizer::PhysicalOptimizer, physical_plan::PhysicalPlan,
};

pub mod heuristic;
pub mod operator;
pub mod physical_optimizer;
pub mod physical_plan;

pub struct Optimizer {
    hep_optimizer: HepOptimizer,
    physical_optimizer: PhysicalOptimizer,
}
impl Optimizer {
    pub fn new() -> Self {
        Self {
            hep_optimizer: HepOptimizer {},
            physical_optimizer: PhysicalOptimizer {},
        }
    }

    pub fn find_best(&self, logical_plan: LogicalPlan) -> PhysicalPlan {
        // optimize logical plan
        let optimized_logical_plan = self.hep_optimizer.find_best(logical_plan);

        // optimize physical plan
        self.physical_optimizer.find_best(optimized_logical_plan)
    }
}
