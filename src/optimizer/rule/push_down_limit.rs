use crate::{
    binder::table_ref::join::JoinType,
    optimizer::heuristic::{
        graph::{HepGraph, HepNodeId},
        pattern::{Pattern, PatternChildrenPredicate},
        rule::Rule,
    },
    planner::operator::{limit::LogicalLimitOperator, LogicalOperator},
};

lazy_static::lazy_static! {
    static ref LIMIT_PROJECT_TRANSPOSE_RULE_PATTERN: Pattern = {
        Pattern {
            predicate: |op| matches!(op, LogicalOperator::Limit(_)),
            children: PatternChildrenPredicate::Predicate(vec![Pattern {
                predicate: |op| matches!(op, LogicalOperator::Project(_)),
                children: PatternChildrenPredicate::None,
            }]),
        }
    };
    static ref ELIMINATE_LIMITS_RULE_PATTERN: Pattern = {
        Pattern {
            predicate: |op| matches!(op, LogicalOperator::Limit(_)),
            children: PatternChildrenPredicate::Predicate(vec![Pattern {
                predicate: |op| matches!(op, LogicalOperator::Limit(_)),
                children: PatternChildrenPredicate::None,
            }]),
        }
    };
    static ref PUSH_LIMIT_THROUGH_JOIN_RULE_PATTERN: Pattern = {
        Pattern {
            predicate: |op| matches!(op, LogicalOperator::Limit(_)),
            children: PatternChildrenPredicate::Predicate(vec![Pattern {
                predicate: |op| matches!(op, LogicalOperator::Join(_)),
                children: PatternChildrenPredicate::None,
            }]),
        }
    };
    static ref PUSH_LIMIT_INTO_SCAN_RULE_PATTERN: Pattern = {
        Pattern {
            predicate: |op| matches!(op, LogicalOperator::Limit(_)),
            children: PatternChildrenPredicate::Predicate(vec![Pattern {
                predicate: |op| matches!(op, LogicalOperator::Scan(_)),
                children: PatternChildrenPredicate::None,
            }]),
        }
    };
}

/// Push down `Limit` past a `Project`.
#[derive(Debug, Clone)]
pub struct LimitProjectTranspose;
impl Rule for LimitProjectTranspose {
    fn pattern(&self) -> &Pattern {
        &LIMIT_PROJECT_TRANSPOSE_RULE_PATTERN
    }
    fn apply(&self, node_id: HepNodeId, graph: &mut HepGraph) -> bool {
        graph.swap_node(node_id, graph.children_at(node_id)[0]);
        true
    }
}

/// Combines two adjacent Limit operators into one, merging the expressions into one single expression.
#[derive(Debug, Clone)]
pub struct EliminateLimits;
impl Rule for EliminateLimits {
    fn pattern(&self) -> &Pattern {
        &ELIMINATE_LIMITS_RULE_PATTERN
    }
    fn apply(&self, node_id: HepNodeId, graph: &mut HepGraph) -> bool {
        if let Some(LogicalOperator::Limit(op)) = graph.operator(node_id) {
            let child_id = graph.children_at(node_id)[0];
            if let Some(LogicalOperator::Limit(child_op)) = graph.operator(child_id) {
                let new_limit_op = LogicalLimitOperator {
                    offset: Some(op.offset.unwrap_or(0) + child_op.offset.unwrap_or(0)),
                    limit: std::cmp::min(op.limit, child_op.limit),
                };

                graph.remove_node(child_id, false);
                graph.replace_node(node_id, LogicalOperator::Limit(new_limit_op));
                return true;
            }
        }
        return false;
    }
}

/// Add extra limits below JOIN:
/// 1. For LEFT OUTER and RIGHT OUTER JOIN, we push limits to the left and right sides, respectively.
/// 2. For FULL OUTER, INNER and CROSS JOIN, we push limits to both the left and right sides if join condition is empty.
#[derive(Debug, Clone)]
pub struct PushLimitThroughJoin;
impl Rule for PushLimitThroughJoin {
    fn pattern(&self) -> &Pattern {
        &PUSH_LIMIT_THROUGH_JOIN_RULE_PATTERN
    }
    fn apply(&self, node_id: HepNodeId, graph: &mut HepGraph) -> bool {
        let child_id = graph.children_at(node_id)[0];
        let (join_type, condition) =
            if let Some(LogicalOperator::Join(op)) = graph.operator(child_id) {
                (Some(op.join_type), op.condition.clone())
            } else {
                (None, None)
            };

        if let Some(join_type) = join_type {
            let grandson_ids = match join_type {
                JoinType::LeftOuter => vec![graph.children_at(child_id)[0]],
                JoinType::RightOuter => vec![graph.children_at(child_id)[1]],
                JoinType::FullOuter | JoinType::CrossJoin | JoinType::Inner => {
                    if condition.is_none() {
                        vec![
                            graph.children_at(child_id)[0],
                            graph.children_at(child_id)[1],
                        ]
                    } else {
                        vec![]
                    }
                }
            };
            let limit_op = graph.remove_node(node_id, false).unwrap();

            for grandson_id in grandson_ids {
                graph.insert_node(child_id, Some(grandson_id), limit_op.clone());
            }
        }
        unimplemented!()
    }
}

/// Push down `Limit` into `Scan`.
#[derive(Debug, Clone)]
pub struct PushLimitIntoScan;
impl Rule for PushLimitIntoScan {
    fn pattern(&self) -> &Pattern {
        &PUSH_LIMIT_INTO_SCAN_RULE_PATTERN
    }
    fn apply(&self, node_id: HepNodeId, graph: &mut HepGraph) -> bool {
        // TODO nees scan operator to support limit
        unimplemented!()
    }
}

mod tests {
    use std::sync::Arc;

    use crate::{
        binder::expression::{column_ref::BoundColumnRef, BoundExpression},
        catalog::column::{Column, ColumnFullName, DataType},
        database::Database,
        optimizer::heuristic::{batch::HepBatchStrategy, rule::Rule, HepOptimizer},
        planner::{
            logical_plan::{self, LogicalPlan},
            operator::LogicalOperator,
        },
    };

    #[test]
    pub fn test_limit_project_transpose() {
        // TODO not manually build plan until subquery is supported
        let logical_plan = LogicalPlan {
            operator: LogicalOperator::new_scan_operator(
                1,
                vec![Column::new(None, "a".to_string(), DataType::Integer, 0)],
            ),
            children: vec![],
        };
        let logical_plan = LogicalPlan {
            operator: LogicalOperator::new_project_operator(vec![BoundExpression::ColumnRef(
                BoundColumnRef {
                    col_name: ColumnFullName::new(None, "a".to_string()),
                },
            )]),
            children: vec![Arc::new(logical_plan)],
        };
        let logical_plan = LogicalPlan {
            operator: LogicalOperator::new_limit_operator(Some(10), None),
            children: vec![Arc::new(logical_plan)],
        };
        let mut optimizer = HepOptimizer::new(logical_plan).batch(
            "limit_project_transpose",
            HepBatchStrategy::once_topdown(),
            vec![Box::new(super::LimitProjectTranspose)],
        );
        let optimized_plan = optimizer.find_best();

        assert!(matches!(
            optimized_plan.operator,
            LogicalOperator::Project(_)
        ));
        assert!(matches!(
            optimized_plan.children[0].operator,
            LogicalOperator::Limit(_)
        ));
        assert!(matches!(
            optimized_plan.children[0].children[0].operator,
            LogicalOperator::Scan(_)
        ));
    }
}
