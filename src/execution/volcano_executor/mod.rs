use std::sync::Arc;

use crate::{optimizer::operator::PhysicalPlanV2, storage::tuple::Tuple};

use super::{execution_plan::ExecutionPlan, ExecutionContext};

pub mod create_table;
pub mod filter;
pub mod insert;
pub mod limit;
pub mod nested_loop_join;
pub mod project;
pub mod table_scan;
pub mod values;

#[derive(Debug, Clone)]
pub struct NextResult {
    pub tuple: Option<Tuple>,
    pub exhausted: bool,
}
impl NextResult {
    pub fn new(tuple: Option<Tuple>, exhausted: bool) -> Self {
        Self { tuple, exhausted }
    }
}

pub trait VolcanoExecutor {
    fn init(
        &self,
        context: &mut ExecutionContext,
        op: Arc<PhysicalPlanV2>,
        children: Vec<Arc<ExecutionPlan>>,
    );
    fn next(
        &self,
        context: &mut ExecutionContext,
        op: Arc<PhysicalPlanV2>,
        children: Vec<Arc<ExecutionPlan>>,
    ) -> NextResult;
}
