use crate::catalog::{Column, DataType, Schema};
use crate::common::ScalarValue;
use crate::expression::{Expr, ExprTrait};
use crate::{BustubxError, BustubxResult, Tuple};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AggregateFunction {
    /// the function kind
    pub func_kind: AggregateFunctionKind,
    /// List of expressions to feed to the functions as arguments
    pub args: Vec<Expr>,
    /// Whether this is a DISTINCT aggregation or not
    pub distinct: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AggregateFunctionKind {
    Count,
}

impl ExprTrait for AggregateFunction {
    fn data_type(&self, _input_schema: &Schema) -> BustubxResult<DataType> {
        match self.func_kind {
            AggregateFunctionKind::Count => Ok(DataType::Int64),
        }
    }

    fn nullable(&self, _input_schema: &Schema) -> BustubxResult<bool> {
        Ok(true)
    }

    fn evaluate(&self, tuple: &Tuple) -> BustubxResult<ScalarValue> {
        match self.func_kind {
            AggregateFunctionKind::Count => {
                let expr = self.args.get(0).ok_or(BustubxError::Internal(format!(
                    "COUNT function should have one arg instead of {:?}",
                    self.args
                )))?;
                expr.evaluate(tuple)
            }
        }
    }

    fn to_column(&self, input_schema: &Schema) -> BustubxResult<Column> {
        Ok(Column::new(
            format!("{}", self),
            self.data_type(input_schema)?,
            self.nullable(input_schema)?,
        ))
    }
}

impl std::fmt::Display for AggregateFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.func_kind)
    }
}

impl std::fmt::Display for AggregateFunctionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
