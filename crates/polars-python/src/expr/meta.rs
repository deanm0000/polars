use polars::prelude::Schema;
use pyo3::prelude::*;

use crate::PyExpr;
use crate::error::PyPolarsErr;
use crate::expr::ToPyExprs;
use crate::prelude::Wrap;

#[pymethods]
impl PyExpr {
    fn meta_eq(&self, other: Self) -> bool {
        self.inner == other.inner
    }

    fn meta_pop(&self, schema: Option<Wrap<Schema>>) -> PyResult<Vec<Self>> {
        let schema = schema.as_ref().map(|s| &s.0);
        let exprs = self
            .inner
            .clone()
            .meta()
            .pop(schema)
            .map_err(PyPolarsErr::from)?;
        Ok(exprs.to_pyexprs())
    }

    fn meta_root_names(&self) -> Vec<String> {
        self.inner
            .clone()
            .meta()
            .root_names()
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    fn meta_output_name(&self) -> PyResult<String> {
        let name = self
            .inner
            .clone()
            .meta()
            .output_name()
            .map_err(PyPolarsErr::from)?;
        Ok(name.to_string())
    }

    fn meta_undo_aliases(&self) -> Self {
        self.inner.clone().meta().undo_aliases().into()
    }

    fn meta_has_multiple_outputs(&self) -> bool {
        self.inner.clone().meta().has_multiple_outputs()
    }

    fn meta_is_column(&self) -> bool {
        self.inner.clone().meta().is_column()
    }

    fn meta_is_regex_projection(&self) -> bool {
        self.inner.clone().meta().is_regex_projection()
    }

    fn meta_is_column_selection(&self, allow_aliasing: bool) -> bool {
        self.inner
            .clone()
            .meta()
            .is_column_selection(allow_aliasing)
    }

    fn meta_is_literal(&self, allow_aliasing: bool) -> bool {
        self.inner.clone().meta().is_literal(allow_aliasing)
    }

    fn _meta_selector_add(&self, other: PyExpr) -> PyResult<PyExpr> {
        let out = self
            .inner
            .clone()
            .meta()
            ._selector_add(other.inner)
            .map_err(PyPolarsErr::from)?;
        Ok(out.into())
    }

    fn _meta_selector_and(&self, other: PyExpr) -> PyResult<PyExpr> {
        let out = self
            .inner
            .clone()
            .meta()
            ._selector_and(other.inner)
            .map_err(PyPolarsErr::from)?;
        Ok(out.into())
    }

    fn _meta_selector_sub(&self, other: PyExpr) -> PyResult<PyExpr> {
        let out = self
            .inner
            .clone()
            .meta()
            ._selector_sub(other.inner)
            .map_err(PyPolarsErr::from)?;
        Ok(out.into())
    }

    fn _meta_selector_xor(&self, other: PyExpr) -> PyResult<PyExpr> {
        let out = self
            .inner
            .clone()
            .meta()
            ._selector_xor(other.inner)
            .map_err(PyPolarsErr::from)?;
        Ok(out.into())
    }

    fn _meta_as_selector(&self) -> PyExpr {
        self.inner.clone().meta()._into_selector().into()
    }

    fn compute_tree_format(
        &self,
        display_as_dot: bool,
        schema: Option<Wrap<Schema>>,
    ) -> Result<String, PyErr> {
        let e = self
            .inner
            .clone()
            .meta()
            .into_tree_formatter(display_as_dot, schema.as_ref().map(|s| &s.0))
            .map_err(PyPolarsErr::from)?;
        Ok(format!("{e}"))
    }

    fn meta_tree_format(&self, schema: Option<Wrap<Schema>>) -> PyResult<String> {
        self.compute_tree_format(false, schema)
    }

    fn meta_show_graph(&self, schema: Option<Wrap<Schema>>) -> PyResult<String> {
        self.compute_tree_format(true, schema)
    }
}
