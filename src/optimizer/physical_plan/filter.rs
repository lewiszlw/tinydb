use std::sync::Arc;

use crate::{
    binder::expression::BoundExpression,
    catalog::schema::Schema,
    dbtype::value::Value,
    execution::{ExecutionContext, VolcanoExecutor},
    storage::tuple::Tuple,
};

use super::PhysicalPlan;

#[derive(derive_new::new, Debug)]
pub struct PhysicalFilter {
    pub predicate: BoundExpression,
    pub input: Arc<PhysicalPlan>,
}
impl PhysicalFilter {
    pub fn output_schema(&self) -> Schema {
        self.input.output_schema()
    }
}
impl VolcanoExecutor for PhysicalFilter {
    fn init(&self, context: &mut ExecutionContext) {
        println!("init filter executor");
        self.input.init(context);
    }
    fn next(&self, context: &mut ExecutionContext) -> Option<Tuple> {
        loop {
            let next_tuple = self.input.next(context);
            if next_tuple.is_none() {
                return None;
            }
            let tuple = next_tuple.unwrap();
            let output_schema = self.input.output_schema();
            let compare_res = self.predicate.evaluate(Some(&tuple), Some(&output_schema));
            if let Value::Boolean(v) = compare_res {
                if v {
                    return Some(tuple);
                }
            } else {
                panic!("filter predicate should be boolean")
            }
        }
    }
}
