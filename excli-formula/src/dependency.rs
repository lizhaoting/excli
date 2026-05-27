use std::collections::{HashMap, HashSet, VecDeque};

use excli_core::error::{ExcliError, ExcliResult};
use excli_core::reference::Reference;
use excli_core::formula::FormulaAst;

/// Directed Acyclic Graph for formula dependency tracking.
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// Maps cell key -> cells this cell depends on (precedents)
    precedents: HashMap<String, Vec<String>>,
    /// Maps cell key -> cells that depend on this cell (dependents)
    dependents: HashMap<String, Vec<String>>,
    /// All registered cells
    cells: HashSet<String>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a cell with its formula AST.
    pub fn register(&mut self, cell_key: &str, ast: &FormulaAst) {
        self.cells.insert(cell_key.to_string());

        let mut refs = Vec::new();
        for r in &ast.references {
            match r {
                Reference::Cell(c) => {
                    let key = cell_key_for(c.sheet.as_deref(), c.column, c.row);
                    refs.push(key);
                }
                Reference::Range(range) => {
                    let start_sheet = range.start.sheet.as_deref();
                    let end_sheet = range.end.sheet.as_deref();
                    for ref_cell in range.iter_cells() {
                        let key = cell_key_for(
                            start_sheet.or(end_sheet),
                            ref_cell.column,
                            ref_cell.row,
                        );
                        refs.push(key);
                    }
                }
                Reference::Named(_) => {
                    refs.push("__named_range__".to_string());
                }
                Reference::Structured(_, _) => {
                    refs.push("__structured_ref__".to_string());
                }
            }
        }

        self.precedents
            .insert(cell_key.to_string(), refs.clone());

        // Update dependents
        for dep in &refs {
            self.dependents
                .entry(dep.clone())
                .or_default()
                .push(cell_key.to_string());
        }
    }

    /// Register a cell without formula (just value).
    pub fn register_value(&mut self, cell_key: &str) {
        self.cells.insert(cell_key.to_string());
    }

    /// Get all cells that this cell depends on (precedents).
    pub fn precedents_of(&self, cell_key: &str) -> Vec<&String> {
        self.precedents.get(cell_key).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Get all cells that depend on this cell (dependents).
    pub fn dependents_of(&self, cell_key: &str) -> Vec<&String> {
        self.dependents.get(cell_key).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Compute topological order for calculation.
    /// Returns cells in order such that precedents come before dependents.
    pub fn topological_sort(&self) -> ExcliResult<Vec<String>> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Compute in-degrees
        for cell in &self.cells {
            // In-degree = number of precedents
            let deg = self.precedents.get(cell).map(|v| v.len()).unwrap_or(0);
            in_degree.insert(cell, deg);
            if deg == 0 {
                queue.push_back(cell.as_str());
            }
        }

        // Kahn's algorithm
        while let Some(cell) = queue.pop_front() {
            result.push(cell.to_string());

            if let Some(deps) = self.dependents.get(cell) {
                for dep in deps {
                    if let Some(deg) = in_degree.get_mut(dep.as_str()) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        if result.len() != self.cells.len() {
            return Err(ExcliError::circular_reference());
        }

        Ok(result)
    }

    /// Detect circular references in the graph.
    pub fn detect_circular(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();

        for cell in &self.cells {
            if !visited.contains(cell.as_str()) {
                let mut path = Vec::new();
                self.dfs_circular(
                    cell,
                    &mut visited,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_circular(
        &self,
        current: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        if path.contains(&current.to_string()) {
            // Found a cycle
            let idx = path.iter().position(|x| x == current).unwrap();
            let cycle: Vec<String> = path[idx..].to_vec();
            cycles.push(cycle);
            return;
        }

        path.push(current.to_string());
        visited.insert(current.to_string());

        if let Some(precs) = self.precedents.get(current) {
            for dep in precs {
                if self.cells.contains(dep.as_str()) {
                    self.dfs_circular(dep, visited, path, cycles);
                }
            }
        }

        path.pop();
    }

    /// Get all cells in the graph.
    pub fn all_cells(&self) -> &HashSet<String> {
        &self.cells
    }

    /// Clear all data.
    pub fn clear(&mut self) {
        self.precedents.clear();
        self.dependents.clear();
        self.cells.clear();
    }
}

/// Create a cell key from sheet, col, row (in A1 notation).
fn cell_key_for(sheet: Option<&str>, col: u32, row: u32) -> String {
    let cell_ref = format!("{}{}", excli_core::reference::column_index_to_name(col), row + 1);
    if let Some(s) = sheet {
        format!("{}!{}", s, cell_ref)
    } else {
        cell_ref
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::formula::AstNode;

    #[test]
    fn test_simple_dependency() {
        let mut graph = DependencyGraph::new();
        let ast_b1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(0, 0), // A1
        )));

        graph.register("B1", &ast_b1);
        graph.register_value("A1");

        assert!(!graph.precedents_of("B1").is_empty());
        assert!(!graph.dependents_of("A1").is_empty());
    }

    #[test]
    fn test_topological_sort() {
        let mut graph = DependencyGraph::new();

        // A1 (value), B1 = A1*2, C1 = B1+A1, D1 = C1*3
        // Correct order: A1 → B1 → C1 → D1
        let ast_b1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(0, 0),
        )));
        let ast_c1 = FormulaAst::new(AstNode::BinaryOp {
            op: excli_core::formula::BinOp::Add,
            left: Box::new(AstNode::Reference(Reference::Cell(
                excli_core::reference::CellReference::new(1, 0),
            ))),
            right: Box::new(AstNode::Reference(Reference::Cell(
                excli_core::reference::CellReference::new(0, 0),
            ))),
        });

        graph.register("B1", &ast_b1);
        graph.register("C1", &ast_c1);
        graph.register_value("A1");

        // D1 = C1 * 3
        let ast_d1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(2, 0),
        )));

        graph.register("D1", &ast_d1);

        let order = graph.topological_sort().unwrap();
        let a1_pos = order.iter().position(|c| c == "A1").unwrap();
        let b1_pos = order.iter().position(|c| c == "B1").unwrap();
        let c1_pos = order.iter().position(|c| c == "C1").unwrap();
        let d1_pos = order.iter().position(|c| c == "D1").unwrap();

        assert!(a1_pos < b1_pos, "A1 should come before B1");
        assert!(b1_pos < c1_pos, "B1 should come before C1");
        assert!(c1_pos < d1_pos, "C1 should come before D1");
    }

    #[test]
    fn test_circular_detection() {
        let mut graph = DependencyGraph::new();

        // A1 → B1 → A1 (circular)
        let ast_a1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(1, 0),
        )));
        let ast_b1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(0, 0),
        )));

        graph.register("A1", &ast_a1);
        graph.register("B1", &ast_b1);

        let cycles = graph.detect_circular();
        assert!(!cycles.is_empty(), "Should detect circular reference");

        let result = graph.topological_sort();
        assert!(result.is_err(), "Topological sort should fail on circular deps");
    }

    #[test]
    fn test_no_circular() {
        let mut graph = DependencyGraph::new();

        let ast_b1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(0, 0),
        )));

        graph.register("B1", &ast_b1);
        graph.register_value("A1");

        let cycles = graph.detect_circular();
        assert!(cycles.is_empty());

        let result = graph.topological_sort();
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_dependents_on_one_cell() {
        let mut graph = DependencyGraph::new();

        // A1 is a value; B1, C1, D1 all depend on A1
        let ast_ref_a1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(0, 0), // A1
        )));

        graph.register_value("A1");
        graph.register("B1", &ast_ref_a1);
        graph.register("C1", &ast_ref_a1);
        graph.register("D1", &ast_ref_a1);

        let deps = graph.dependents_of("A1");
        assert_eq!(deps.len(), 3, "A1 should have 3 dependents");

        let prec_b = graph.precedents_of("B1");
        assert_eq!(prec_b.len(), 1);
        assert!(prec_b.iter().any(|k| k.as_str() == "A1"));
    }

    #[test]
    fn test_clear_graph() {
        let mut graph = DependencyGraph::new();

        graph.register_value("A1");
        let ast = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(0, 0),
        )));
        graph.register("B1", &ast);

        assert_eq!(graph.all_cells().len(), 2);

        graph.clear();
        assert!(graph.all_cells().is_empty());
        assert!(graph.precedents_of("B1").is_empty());
        assert!(graph.dependents_of("A1").is_empty());
    }

    #[test]
    fn test_register_value_only() {
        let mut graph = DependencyGraph::new();
        graph.register_value("X1");
        graph.register_value("Y1");

        assert_eq!(graph.all_cells().len(), 2);
        // Value-only cells have no precedents
        assert!(graph.precedents_of("X1").is_empty());
        // No topological issue with pure values
        let order = graph.topological_sort().unwrap();
        assert_eq!(order.len(), 2);
    }

    #[test]
    fn test_no_self_reference_in_non_circular() {
        let mut graph = DependencyGraph::new();
        // B1 depends on A1, C1 depends on B1 — no cycles
        let ast_b1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(0, 0), // A1
        )));
        let ast_c1 = FormulaAst::new(AstNode::Reference(Reference::Cell(
            excli_core::reference::CellReference::new(1, 0), // B1
        )));

        graph.register_value("A1");
        graph.register("B1", &ast_b1);
        graph.register("C1", &ast_c1);

        let cycles = graph.detect_circular();
        assert!(cycles.is_empty(), "Should have no circular references");
    }
}

