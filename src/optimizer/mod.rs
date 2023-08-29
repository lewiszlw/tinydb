use std::sync::Arc;

use crate::planner::{
    logical_plan::{self, LogicalPlan},
    operator::LogicalOperator,
};

use self::{
    heuristic::HepOptimizer, physical_optimizer::PhysicalOptimizer, physical_plan::PhysicalPlan,
    physical_plan_v2::PhysicalPlanV2,
};

pub mod heuristic;
pub mod physical_optimizer;
pub mod physical_plan;
pub mod physical_plan_v2;
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
        // optimize logical plan
        let optimized_logical_plan = self.hep_optimizer.find_best();

        // optimize physical plan
        self.physical_optimizer.find_best(optimized_logical_plan)
    }

    pub fn find_best_v2(&mut self) -> PhysicalPlanV2 {
        // optimize logical plan
        let optimized_logical_plan = self.hep_optimizer.find_best();

        // optimize physical plan
        self.physical_optimizer.find_best_v2(optimized_logical_plan)
    }
}
