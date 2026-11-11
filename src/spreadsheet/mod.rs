//! Small, UI-independent formula evaluator for canvas tables.
//!
//! Formula source is kept in the document. This module computes display values
//! from table grids and explicit formula-cell coordinates without mutating them.

pub mod catalog;
mod parser;

use parser::{BinaryOp, Expr, UnaryOp, parse_formula};
use std::collections::{HashMap, HashSet};
#[cfg(not(windows))]
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellCoord {
    pub row: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellAddress {
    pub table_id: u64,
    pub coord: CellCoord,
}

/// Input view over a document table. Coordinates are zero-based.
pub struct TableInput<'a> {
    pub id: u64,
    pub cells: &'a [Vec<String>],
    pub formula_cells: &'a HashSet<CellCoord>,
    /// Row indices hidden by the user, before filtering.
    pub manually_hidden_rows: &'a HashSet<usize>,
    /// Row indices excluded by the table's saved filter criteria.
    pub filtered_out_rows: &'a HashSet<usize>,
    /// Merged rectangles as `(row, column, row_count, column_count)` tuples.
    pub merged_cells: &'a [(usize, usize, usize, usize)],
}

#[derive(Clone, Debug, PartialEq)]
pub enum CalcError {
    Parse,
    Name,
    Ref,
    Div0,
    Value,
    Num,
    NotAvailable,
    Calc,
    Spill,
    Cycle,
}

impl CalcError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Parse => "#ERROR!",
            Self::Name => "#NAME?",
            Self::Ref => "#REF!",
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Num => "#NUM!",
            Self::NotAvailable => "#N/A",
            Self::Calc => "#CALC!",
            Self::Spill => "#SPILL!",
            Self::Cycle => "#CYCLE!",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Blank,
    Number(f64),
    DateTime(f64),
    Text(String),
    Bool(bool),
    Array(ArrayValue),
    Error(CalcError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayValue {
    pub rows: usize,
    pub columns: usize,
    pub values: Vec<Value>,
}

impl ArrayValue {
    fn get(&self, row: usize, column: usize) -> Option<&Value> {
        if row >= self.rows || column >= self.columns {
            return None;
        }
        self.values.get(row * self.columns + column)
    }
}

impl Value {
    pub fn display_text(&self) -> String {
        match self {
            Self::Blank => String::new(),
            Self::Number(n) => format_number(*n),
            Self::DateTime(n) => format_datetime_default(*n),
            Self::Text(s) => s.clone(),
            Self::Bool(v) => if *v { "TRUE" } else { "FALSE" }.to_owned(),
            Self::Array(array) => array
                .values
                .first()
                .map(Value::display_text)
                .unwrap_or_default(),
            Self::Error(e) => e.code().to_owned(),
        }
    }

    fn as_number(&self) -> Result<f64, CalcError> {
        match self {
            Self::Number(n) if n.is_finite() => Ok(*n),
            Self::Number(_) => Err(CalcError::Num),
            Self::DateTime(n) if n.is_finite() => Ok(*n),
            Self::DateTime(_) => Err(CalcError::Num),
            Self::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
            Self::Blank => Ok(0.0),
            Self::Text(s) if s.trim().is_empty() => Ok(0.0),
            Self::Text(s) => s.trim().parse::<f64>().map_err(|_| CalcError::Value),
            Self::Error(e) => Err(e.clone()),
            Self::Array(_) => Err(CalcError::Value),
        }
    }

    fn as_bool(&self) -> Result<bool, CalcError> {
        match self {
            Self::Bool(v) => Ok(*v),
            Self::Number(n) => Ok(*n != 0.0),
            Self::DateTime(n) => Ok(*n != 0.0),
            Self::Blank => Ok(false),
            Self::Text(s) if s.is_empty() => Ok(false),
            Self::Text(_) => Err(CalcError::Value),
            Self::Error(e) => Err(e.clone()),
            Self::Array(_) => Err(CalcError::Value),
        }
    }

    fn as_text(&self) -> Result<String, CalcError> {
        match self {
            Self::Text(s) => Ok(s.clone()),
            Self::Blank => Ok(String::new()),
            Self::Error(e) => Err(e.clone()),
            Self::Array(_) => Err(CalcError::Value),
            v => Ok(v.display_text()),
        }
    }
}

#[derive(Default)]
pub struct WorkbookResult {
    pub values: HashMap<CellAddress, Value>,
    pub errors: Vec<(CellAddress, CalcError)>,
    pub spills: HashMap<CellAddress, ArrayValue>,
    pub spill_origins: HashMap<CellAddress, CellAddress>,
}

impl WorkbookResult {
    pub fn value_at(&self, table_id: u64, row: usize, column: usize) -> Option<&Value> {
        self.values.get(&CellAddress {
            table_id,
            coord: CellCoord { row, column },
        })
    }

    pub fn error_at(&self, table_id: u64, row: usize, column: usize) -> Option<&CalcError> {
        let address = CellAddress {
            table_id,
            coord: CellCoord { row, column },
        };
        self.errors
            .iter()
            .find_map(|(cell, error)| (*cell == address).then_some(error))
    }

    pub fn display_value_at(&self, table_id: u64, row: usize, column: usize) -> Option<&Value> {
        let address=CellAddress{table_id,coord:CellCoord{row,column}};
        if let Some(origin)=self.spill_origins.get(&address) {
            let array=self.spills.get(origin)?;
            return array.get(row.saturating_sub(origin.coord.row),column.saturating_sub(origin.coord.column));
        }
        self.value_at(table_id, row, column)
    }

    pub fn spill_origin_at(&self, table_id: u64, row: usize, column: usize) -> Option<CellAddress> {
        self.spill_origins
            .get(&CellAddress {
                table_id,
                coord: CellCoord { row, column },
            })
            .copied()
    }
}

/// Returns the cell's visible literal text, hiding an apostrophe used to force
/// a formula-looking value to remain text (`'=SUM(A1:A2)` -> `=SUM(A1:A2)`).
pub fn display_literal(raw: &str) -> String {
    raw.strip_prefix("'=")
        .map(|tail| format!("={tail}"))
        .unwrap_or_else(|| raw.to_owned())
}

/// Adjusts relative A1 references when a formula is filled/copied by a delta.
/// `$`-anchored dimensions stay fixed. Table-qualified references keep their
/// table ID while their relative row/column coordinates are translated.
pub fn translate_formula(
    source: &str,
    row_delta: i32,
    column_delta: i32,
    _current_table_id: u64,
) -> Result<String, CalcError> {
    let mut expr = parse_formula(source)?;
    translate_expr(&mut expr, row_delta, column_delta)?;
    Ok(format!("={}", format_expr(&expr)))
}

/// Remaps explicit references to a source table when that table is duplicated.
pub fn remap_table_reference(source: &str, old_id: u64, new_id: u64) -> Result<String, CalcError> {
    let mut expr = parse_formula(source)?;
    remap_expr(&mut expr, old_id, new_id);
    Ok(format!("={}", format_expr(&expr)))
}

fn remap_expr(expr: &mut Expr, old_id: u64, new_id: u64) {
    match expr {
        Expr::Ref(r) => remap_ref(r, old_id, new_id),
        Expr::Range(a, b) => {
            remap_ref(a, old_id, new_id);
            remap_ref(b, old_id, new_id);
        }
        Expr::Unary(_, inner) => remap_expr(inner, old_id, new_id),
        Expr::Binary(_, left, right) => {
            remap_expr(left, old_id, new_id);
            remap_expr(right, old_id, new_id);
        }
        Expr::Call(_, args) => {
            for arg in args {
                remap_expr(arg, old_id, new_id);
            }
        }
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) => {}
    }
}

fn remap_ref(r: &mut parser::CellRef, old_id: u64, new_id: u64) {
    if r.table_id == Some(old_id) {
        r.table_id = Some(new_id);
    }
}

fn translate_expr(expr: &mut Expr, dr: i32, dc: i32) -> Result<(), CalcError> {
    match expr {
        Expr::Ref(r) => translate_ref(r, dr, dc),
        Expr::Range(a, b) => {
            translate_ref(a, dr, dc)?;
            translate_ref(b, dr, dc)
        }
        Expr::Unary(_, inner) => translate_expr(inner, dr, dc),
        Expr::Binary(_, left, right) => {
            translate_expr(left, dr, dc)?;
            translate_expr(right, dr, dc)
        }
        Expr::Call(_, args) => {
            for arg in args {
                translate_expr(arg, dr, dc)?;
            }
            Ok(())
        }
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) => Ok(()),
    }
}

fn translate_ref(r: &mut parser::CellRef, dr: i32, dc: i32) -> Result<(), CalcError> {
    if !r.abs_row {
        r.row = r
            .row
            .checked_add_signed(dr as isize)
            .ok_or(CalcError::Ref)?;
    }
    if !r.abs_column {
        r.column = r
            .column
            .checked_add_signed(dc as isize)
            .ok_or(CalcError::Ref)?;
    }
    if r.row >= 1_048_576 || r.column >= 16_384 {
        return Err(CalcError::Ref);
    }
    Ok(())
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Number(n) => format_number(*n),
        Expr::Text(s) => format!("\"{}\"", s.replace('"', "\"\"")),
        Expr::Bool(v) => if *v { "TRUE" } else { "FALSE" }.to_owned(),
        Expr::Ref(r) => format_ref(r),
        Expr::Range(a, b) => format!("{}:{}", format_ref(a), format_ref(b)),
        Expr::Unary(op, e) => match op {
            UnaryOp::Plus => format!("(+{})", format_expr(e)),
            UnaryOp::Minus => format!("(-{})", format_expr(e)),
            UnaryOp::Percent => format!("({}%)", format_expr(e)),
        },
        Expr::Binary(op, left, right) => {
            let symbol = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Pow => "^",
                BinaryOp::Concat => "&",
                BinaryOp::Eq => "=",
                BinaryOp::Ne => "<>",
                BinaryOp::Lt => "<",
                BinaryOp::Le => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Ge => ">=",
            };
            format!("({}{}{})", format_expr(left), symbol, format_expr(right))
        }
        Expr::Call(name, args) => format!(
            "{}({})",
            name,
            args.iter().map(format_expr).collect::<Vec<_>>().join(",")
        ),
    }
}

fn format_ref(r: &parser::CellRef) -> String {
    let mut out = String::new();
    if let Some(id) = r.table_id {
        out.push_str(&format!("{id}:"));
    }
    if r.abs_column {
        out.push('$');
    }
    out.push_str(&column_name(r.column));
    if r.abs_row {
        out.push('$');
    }
    out.push_str(&(r.row + 1).to_string());
    out
}

fn column_name(mut n: usize) -> String {
    n += 1;
    let mut bytes = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        bytes.push(b'A' + rem as u8);
        n = (n - 1) / 26;
    }
    bytes.reverse();
    String::from_utf8(bytes).unwrap_or_default()
}

pub fn evaluate_workbook(tables: &[TableInput<'_>]) -> WorkbookResult {
    let mut engine = Engine::new(tables);
    let mut addresses = Vec::new();
    for table in tables {
        for (row, cells) in table.cells.iter().enumerate() {
            for column in 0..cells.len() {
                addresses.push(CellAddress {
                    table_id: table.id,
                    coord: CellCoord { row, column },
                });
            }
        }
    }
    for address in addresses {
        let _ = engine.eval_cell(address);
    }
    let (spills, spill_origins) = resolve_spills(&mut engine, tables);
    let errors = engine
        .values
        .iter()
        .filter_map(|(address, value)| {
            if let Value::Error(err) = value {
                Some((*address, err.clone()))
            } else {
                None
            }
        })
        .collect();
    WorkbookResult {
        values: engine.values,
        errors,
        spills,
        spill_origins,
    }
}

fn resolve_spills(
    engine: &mut Engine<'_>,
    tables: &[TableInput<'_>],
) -> (
    HashMap<CellAddress, ArrayValue>,
    HashMap<CellAddress, CellAddress>,
) {
    let input_by_id = tables
        .iter()
        .map(|table| (table.id, table))
        .collect::<HashMap<_, _>>();
    let candidates = engine
        .values
        .iter()
        .filter_map(|(address, value)| {
            if let Value::Array(array) = value {
                input_by_id
                    .get(&address.table_id)
                    .filter(|table| table.formula_cells.contains(&address.coord))
                    .map(|_| (*address, array.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut blocked = HashSet::new();
    for (anchor, array) in &candidates {
        let Some(table) = input_by_id.get(&anchor.table_id) else {
            blocked.insert(*anchor);
            continue;
        };
        let end_row = anchor.coord.row.checked_add(array.rows);
        let end_column = anchor.coord.column.checked_add(array.columns);
        let (Some(end_row), Some(end_column)) = (end_row, end_column) else {
            blocked.insert(*anchor);
            continue;
        };
        if array.rows == 0 || array.columns == 0 || end_row > table.cells.len() {
            blocked.insert(*anchor);
            continue;
        }
        let mut invalid = false;
        'cells: for row in anchor.coord.row..end_row {
            for column in anchor.coord.column..end_column {
                if row == anchor.coord.row && column == anchor.coord.column {
                    continue;
                }
                if table.cells.get(row).and_then(|r| r.get(column)).is_none()
                    || table.cells[row][column].len() > 0
                    || table.formula_cells.contains(&CellCoord { row, column })
                {
                    invalid = true;
                    break 'cells;
                }
            }
        }
        if end_column > table.cells.iter().map(Vec::len).max().unwrap_or(0) {
            invalid = true;
        }
        if table
            .merged_cells
            .iter()
            .any(|(row, column, rows, columns)| {
                rects_overlap(
                    anchor.coord.row,
                    anchor.coord.column,
                    array.rows,
                    array.columns,
                    *row,
                    *column,
                    *rows,
                    *columns,
                )
            })
        {
            invalid = true;
        }
        if invalid {
            blocked.insert(*anchor);
        }
    }
    for i in 0..candidates.len() {
        for j in i + 1..candidates.len() {
            let (a, av) = &candidates[i];
            let (b, bv) = &candidates[j];
            if a.table_id == b.table_id
                && rects_overlap(
                    a.coord.row,
                    a.coord.column,
                    av.rows,
                    av.columns,
                    b.coord.row,
                    b.coord.column,
                    bv.rows,
                    bv.columns,
                )
            {
                blocked.insert(*a);
                blocked.insert(*b);
            }
        }
    }
    let mut spills = HashMap::new();
    let mut origins = HashMap::new();
    for (anchor, array) in candidates {
        if blocked.contains(&anchor) {
            engine.values.insert(anchor, Value::Error(CalcError::Spill));
            continue;
        }
        for row_offset in 0..array.rows {
            for column_offset in 0..array.columns {
                let address = CellAddress {
                    table_id: anchor.table_id,
                    coord: CellCoord {
                        row: anchor.coord.row + row_offset,
                        column: anchor.coord.column + column_offset,
                    },
                };
                if address != anchor {
                    origins.insert(address, anchor);
                }
                if address == anchor {
                    engine.values.insert(address, Value::Array(array.clone()));
                } else if let Some(value) = array.get(row_offset, column_offset) {
                    engine.values.insert(address, value.clone());
                }
            }
        }
        // Keep the array in the evaluator cache so dependent array expressions
        // still receive the full shape, while the public cell value is scalar.
        spills.insert(anchor, array);
    }
    let anchors = spills.keys().copied().collect::<HashSet<_>>();
    let blocked_anchors = blocked;
    for table in tables {
        for coord in table.formula_cells {
            let address = CellAddress {
                table_id: table.id,
                coord: *coord,
            };
            if !anchors.contains(&address) && !blocked_anchors.contains(&address) {
                engine.values.remove(&address);
            }
        }
    }
    let formulas = tables
        .iter()
        .flat_map(|table| {
            table.formula_cells.iter().map(move |coord| CellAddress {
                table_id: table.id,
                coord: *coord,
            })
        })
        .collect::<Vec<_>>();
    for address in formulas {
        let _ = engine.eval_cell(address);
    }
    // Engine cache retains arrays at anchors; expose their top-left element to renderers.
    for (anchor, array) in &spills {
        engine
            .values
            .insert(*anchor, array.get(0, 0).cloned().unwrap_or(Value::Blank));
    }
    (spills, origins)
}

fn rects_overlap(
    ar: usize,
    ac: usize,
    ah: usize,
    aw: usize,
    br: usize,
    bc: usize,
    bh: usize,
    bw: usize,
) -> bool {
    ar < br.saturating_add(bh)
        && br < ar.saturating_add(ah)
        && ac < bc.saturating_add(bw)
        && bc < ac.saturating_add(aw)
}

fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return "#NUM!".to_owned();
    }
    if n == 0.0 {
        return "0".to_owned();
    }
    let mut s = format!("{n:.15}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

struct Engine<'a> {
    tables: HashMap<u64, &'a TableInput<'a>>,
    values: HashMap<CellAddress, Value>,
    visiting: Vec<CellAddress>,
    now_serial: f64,
}

impl<'a> Engine<'a> {
    fn new(inputs: &'a [TableInput<'a>]) -> Self {
        Self {
            tables: inputs.iter().map(|t| (t.id, t)).collect(),
            values: HashMap::new(),
            visiting: Vec::new(),
            now_serial: local_now_serial(),
        }
    }

    fn eval_cell(&mut self, address: CellAddress) -> Value {
        if let Some(value) = self.values.get(&address) {
            return value.clone();
        }
        if self.visiting.contains(&address) {
            let value = Value::Error(CalcError::Cycle);
            self.values.insert(address, value.clone());
            return value;
        }
        let Some(table) = self.tables.get(&address.table_id).copied() else {
            return Value::Error(CalcError::Ref);
        };
        let Some(raw) = table
            .cells
            .get(address.coord.row)
            .and_then(|row| row.get(address.coord.column))
        else {
            return Value::Error(CalcError::Ref);
        };
        if !table.formula_cells.contains(&address.coord) {
            let value = parse_literal(raw);
            self.values.insert(address, value.clone());
            return value;
        }
        self.visiting.push(address);
        let value = match parse_formula(raw) {
            Ok(expr) => self.eval_expr(&expr, address.table_id),
            Err(_) => Value::Error(CalcError::Parse),
        };
        self.visiting.pop();
        self.values.insert(address, value.clone());
        value
    }

    fn eval_expr(&mut self, expr: &Expr, current_table: u64) -> Value {
        match expr {
            Expr::Number(n) => Value::Number(*n),
            Expr::Text(s) => Value::Text(s.clone()),
            Expr::Bool(v) => Value::Bool(*v),
            Expr::Ref(r) => self.eval_ref(r, current_table),
            Expr::Range(a, b) => self.eval_range(a, b, current_table),
            Expr::Unary(op, e) => {
                let v = self.eval_expr(e, current_table);
                apply_unary(*op, v)
            }
            Expr::Binary(op, left, right) => {
                let l = self.eval_expr(left, current_table);
                if let Value::Error(e) = l {
                    return Value::Error(e);
                }
                let r = self.eval_expr(right, current_table);
                eval_binary(*op, l, r)
            }
            Expr::Call(name, args) => self.eval_call(name, args, current_table),
        }
    }

    fn eval_ref(&mut self, r: &parser::CellRef, current_table: u64) -> Value {
        self.eval_cell(CellAddress {
            table_id: r.table_id.unwrap_or(current_table),
            coord: CellCoord {
                row: r.row,
                column: r.column,
            },
        })
    }

    fn eval_range(
        &mut self,
        a: &parser::CellRef,
        b: &parser::CellRef,
        current_table: u64,
    ) -> Value {
        let table_id = a.table_id.or(b.table_id).unwrap_or(current_table);
        if a.table_id.is_some_and(|id| id != table_id)
            || b.table_id.is_some_and(|id| id != table_id)
        {
            return Value::Error(CalcError::Ref);
        }
        let (Some(table),) = (self.tables.get(&table_id),) else {
            return Value::Error(CalcError::Ref);
        };
        let table = *table;
        let max_row = table.cells.len();
        let max_col = table.cells.iter().map(Vec::len).max().unwrap_or(0);
        if a.row >= max_row || b.row >= max_row || a.column >= max_col || b.column >= max_col {
            return Value::Error(CalcError::Ref);
        }
        let first_row = a.row.min(b.row);
        let last_row = a.row.max(b.row);
        let first_column = a.column.min(b.column);
        let last_column = a.column.max(b.column);
        let mut values = Vec::new();
        for row in first_row..=last_row {
            for column in first_column..=last_column {
                let mut value = self.eval_cell(CellAddress {
                    table_id,
                    coord: CellCoord { row, column },
                });
                if let Value::Array(array) = value {
                    value = array.values.first().cloned().unwrap_or(Value::Blank);
                }
                values.push(value);
            }
        }
        Value::Array(ArrayValue {
            rows: last_row - first_row + 1,
            columns: last_column - first_column + 1,
            values,
        })
    }

    fn eval_call(&mut self, name: &str, args: &[Expr], current_table: u64) -> Value {
        let name = name.to_ascii_uppercase();
        match name.as_str() {
            "IF" => {
                if args.len() < 2 || args.len() > 3 {
                    return Value::Error(CalcError::Value);
                }
                let test = self.eval_expr(&args[0], current_table);
                match test.as_bool() {
                    Ok(true) => self.eval_expr(&args[1], current_table),
                    Ok(false) => args
                        .get(2)
                        .map(|e| self.eval_expr(e, current_table))
                        .unwrap_or(Value::Bool(false)),
                    Err(e) => Value::Error(e),
                }
            }
            "IFERROR" => {
                if args.len() != 2 {
                    return Value::Error(CalcError::Value);
                }
                let value = self.eval_expr(&args[0], current_table);
                if matches!(value, Value::Error(_)) {
                    self.eval_expr(&args[1], current_table)
                } else {
                    value
                }
            }
            "IFS" => {
                if args.len() < 2 || args.len() % 2 != 0 {
                    return Value::Error(CalcError::Value);
                }
                for pair in args.chunks_exact(2) {
                    let condition = self.eval_expr(&pair[0], current_table);
                    match condition.as_bool() {
                        Ok(true) => return self.eval_expr(&pair[1], current_table),
                        Ok(false) => {}
                        Err(e) => return Value::Error(e),
                    }
                }
                Value::Error(CalcError::NotAvailable)
            }
            "AND" | "OR" => {
                if args.is_empty() {
                    return Value::Error(CalcError::Value);
                }
                let mut result = name == "AND";
                for arg in args {
                    let v = self.eval_expr(arg, current_table);
                    let b = match v.as_bool() {
                        Ok(b) => b,
                        Err(e) => return Value::Error(e),
                    };
                    if name == "AND" {
                        result &= b;
                    } else {
                        result |= b;
                    }
                }
                Value::Bool(result)
            }
            "NOT" => {
                if args.len() != 1 {
                    return Value::Error(CalcError::Value);
                }
                match self.eval_expr(&args[0], current_table).as_bool() {
                    Ok(b) => Value::Bool(!b),
                    Err(e) => Value::Error(e),
                }
            }
            "SUMIF" | "SUMIFS" | "COUNTIF" | "COUNTIFS" | "AVERAGEIF" | "AVERAGEIFS" => {
                self.eval_criteria_call(&name, args, current_table)
            }
            "SUBTOTAL" => self.eval_subtotal(args, current_table),
            "AGGREGATE" => self.eval_aggregate(args, current_table),
            "FILTER" => self.eval_filter(args, current_table),
            "CONCAT" | "CONCATENATE" | "TEXTJOIN" | "LEFT" | "RIGHT" | "MID" | "LEN" | "FIND"
            | "SEARCH" | "SUBSTITUTE" | "REPLACE" | "TRIM" | "TEXT" => {
                self.eval_text_call(&name, args, current_table)
            }
            "TODAY" | "NOW" | "DATE" | "DATEDIF" | "NETWORKDAYS" => {
                self.eval_date_call(&name, args, current_table)
            }
            "RANK.EQ" => self.eval_rank_eq(args, current_table),
            _ => self.eval_eager_call(&name, args, current_table),
        }
    }

    fn eval_criteria_call(&mut self, name: &str, args: &[Expr], current_table: u64) -> Value {
        let value_args = args
            .iter()
            .map(|arg| self.eval_expr(arg, current_table))
            .collect::<Vec<_>>();
        if let Some(error) = value_args.iter().find_map(value_error) {
            return Value::Error(error);
        }
        let (sum_range, criteria_ranges, criteria_pairs): (&Value, Vec<&Value>, Vec<&Value>) =
            match name {
                "SUMIF" | "COUNTIF" | "AVERAGEIF" => {
                    if !(2..=3).contains(&value_args.len()) {
                        return Value::Error(CalcError::Value);
                    }
                    let criteria_range = &value_args[0];
                    let criteria = &value_args[1];
                    let sum = value_args.get(2).unwrap_or(criteria_range);
                    (sum, vec![criteria_range], vec![criteria])
                }
                "SUMIFS" | "AVERAGEIFS" => {
                    if value_args.len() < 3 || value_args.len() % 2 == 0 {
                        return Value::Error(CalcError::Value);
                    }
                    let ranges = value_args[1..].iter().step_by(2).collect::<Vec<_>>();
                    let criteria = value_args[2..].iter().step_by(2).collect::<Vec<_>>();
                    (&value_args[0], ranges, criteria)
                }
                "COUNTIFS" => {
                    if value_args.len() < 2 || value_args.len() % 2 != 0 {
                        return Value::Error(CalcError::Value);
                    }
                    let ranges = value_args.iter().step_by(2).collect::<Vec<_>>();
                    let criteria = value_args.iter().skip(1).step_by(2).collect::<Vec<_>>();
                    (&value_args[0], ranges, criteria)
                }
                _ => return Value::Error(CalcError::Name),
            };
        let range_values = criteria_ranges
            .iter()
            .map(|v| flatten_values(v).into_iter().cloned().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        if range_values.is_empty() {
            return Value::Error(CalcError::Value);
        }
        let size = range_values[0].len();
        if range_values.iter().any(|r| r.len() != size) {
            return Value::Error(CalcError::Value);
        }
        let sum_values = if name.starts_with("COUNT") {
            Vec::new()
        } else {
            flatten_values(sum_range)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        };
        if !name.starts_with("COUNT") && sum_values.len() != size {
            return Value::Error(CalcError::Value);
        }
        let mut count = 0usize;
        let mut sum = 0.0;
        for idx in 0..size {
            let matches = range_values
                .iter()
                .zip(criteria_pairs.iter())
                .all(|(range, criterion)| criteria_matches(&range[idx], criterion));
            if !matches {
                continue;
            }
            if name.starts_with("COUNT") {
                count += 1;
            } else {
                let value = &sum_values[idx];
                if let Value::Error(e) = value {
                    return Value::Error(e.clone());
                }
                if let Value::Number(n) | Value::DateTime(n) = value {
                    sum += n;
                } else if !matches!(value, Value::Blank | Value::Text(_) | Value::Bool(_)) {
                    return Value::Error(CalcError::Value);
                }
                if name.starts_with("AVERAGE")
                    && matches!(value, Value::Number(_) | Value::DateTime(_))
                {
                    count += 1;
                }
            }
        }
        match name {
            "COUNTIF" | "COUNTIFS" => Value::Number(count as f64),
            "SUMIF" | "SUMIFS" => Value::Number(sum),
            "AVERAGEIF" | "AVERAGEIFS" if count == 0 => Value::Error(CalcError::Div0),
            "AVERAGEIF" | "AVERAGEIFS" => Value::Number(sum / count as f64),
            _ => Value::Error(CalcError::Name),
        }
    }

    fn eval_filter(&mut self, args: &[Expr], current_table: u64) -> Value {
        if !(2..=3).contains(&args.len()) {
            return Value::Error(CalcError::Value);
        }
        let array_value = self.eval_expr(&args[0], current_table);
        if let Value::Error(e) = &array_value {
            return Value::Error(e.clone());
        }
        let include_value = self.eval_expr(&args[1], current_table);
        if let Value::Error(e) = &include_value {
            return Value::Error(e.clone());
        }
        let array = match array_value {
            Value::Array(a) => a,
            value => ArrayValue {
                rows: 1,
                columns: 1,
                values: vec![value],
            },
        };
        let include = match include_value {
            Value::Array(a) => a,
            value => ArrayValue {
                rows: 1,
                columns: 1,
                values: vec![value],
            },
        };
        if include.values.iter().any(|v| matches!(v, Value::Error(_))) {
            return include
                .values
                .iter()
                .find_map(value_error)
                .map(Value::Error)
                .unwrap_or(Value::Error(CalcError::Value));
        }
        let filter_rows = include.columns == 1 && include.rows == array.rows;
        let filter_columns = include.rows == 1 && include.columns == array.columns;
        if !filter_rows && !filter_columns {
            return Value::Error(CalcError::Value);
        }
        let mut selected = Vec::new();
        if filter_rows {
            for row in 0..array.rows {
                let keep = match include
                    .get(row, 0)
                    .ok_or(CalcError::Value)
                    .and_then(Value::as_bool)
                {
                    Ok(v) => v,
                    Err(e) => return Value::Error(e),
                };
                if keep {
                    for column in 0..array.columns {
                        selected.push(array.get(row, column).cloned().unwrap_or(Value::Blank));
                    }
                }
            }
            let selected_rows = if array.columns == 0 {
                0
            } else {
                selected.len() / array.columns
            };
            if selected_rows == 0 {
                return self.filter_empty(args, current_table);
            }
            Value::Array(ArrayValue {
                rows: selected_rows,
                columns: array.columns,
                values: selected,
            })
        } else {
            let mut keep_columns = Vec::new();
            for column in 0..array.columns {
                let keep = match include
                    .get(0, column)
                    .ok_or(CalcError::Value)
                    .and_then(Value::as_bool)
                {
                    Ok(v) => v,
                    Err(e) => return Value::Error(e),
                };
                if keep {
                    keep_columns.push(column);
                }
            }
            let selected_columns = keep_columns.len();
            if selected_columns == 0 {
                return self.filter_empty(args, current_table);
            }
            for row in 0..array.rows {
                for column in &keep_columns {
                    selected.push(array.get(row, *column).cloned().unwrap_or(Value::Blank));
                }
            }
            Value::Array(ArrayValue {
                rows: array.rows,
                columns: selected_columns,
                values: selected,
            })
        }
    }

    fn filter_empty(&mut self, args: &[Expr], current_table: u64) -> Value {
        args.get(2)
            .map(|expr| self.eval_expr(expr, current_table))
            .unwrap_or(Value::Error(CalcError::Calc))
    }

    fn eval_text_call(&mut self, name: &str, args: &[Expr], current_table: u64) -> Value {
        let values = args
            .iter()
            .map(|arg| self.eval_expr(arg, current_table))
            .collect::<Vec<_>>();
        if let Some(error) = values.iter().flat_map(flatten_values).find_map(value_error) {
            return Value::Error(error);
        }
        match name {
            "CONCAT" | "CONCATENATE" => {
                if values.is_empty() {
                    return Value::Error(CalcError::Value);
                }
                let mut out = String::new();
                for value in values.iter().flat_map(flatten_values) {
                    match value.as_text() {
                        Ok(text) => out.push_str(&text),
                        Err(e) => return Value::Error(e),
                    }
                }
                Value::Text(out)
            }
            "TEXTJOIN" => {
                if values.len() < 3 {
                    return Value::Error(CalcError::Value);
                }
                let delimiter = match values[0].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                let ignore_empty = match values[1].as_bool() {
                    Ok(b) => b,
                    Err(e) => return Value::Error(e),
                };
                let mut parts = Vec::new();
                for value in values[2..].iter().flat_map(flatten_values) {
                    let text = match value.as_text() {
                        Ok(s) => s,
                        Err(e) => return Value::Error(e),
                    };
                    if !ignore_empty || !text.is_empty() {
                        parts.push(text);
                    }
                }
                Value::Text(parts.join(&delimiter))
            }
            "LEFT" | "RIGHT" => {
                if !(1..=2).contains(&values.len()) {
                    return Value::Error(CalcError::Value);
                }
                let text = match values[0].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                let count = match optional_char_count(&values, 1) {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                let chars = text.chars().collect::<Vec<_>>();
                let n = count.min(chars.len());
                let selected = if name == "LEFT" {
                    chars[..n].iter().collect()
                } else {
                    chars[chars.len() - n..].iter().collect()
                };
                Value::Text(selected)
            }
            "MID" => {
                if values.len() != 3 {
                    return Value::Error(CalcError::Value);
                }
                let text = match values[0].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                let start = match nonnegative_integer(&values[1], false) {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                let count = match nonnegative_integer(&values[2], true) {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                let chars = text.chars().collect::<Vec<_>>();
                if start == 0 {
                    return Value::Error(CalcError::Value);
                }
                let begin = start.saturating_sub(1).min(chars.len());
                let end = begin.saturating_add(count).min(chars.len());
                Value::Text(chars[begin..end].iter().collect())
            }
            "LEN" => {
                if values.len() != 1 {
                    return Value::Error(CalcError::Value);
                }
                match values[0].as_text() {
                    Ok(s) => Value::Number(s.chars().count() as f64),
                    Err(e) => Value::Error(e),
                }
            }
            "FIND" | "SEARCH" => self.eval_find_search(name, &values),
            "SUBSTITUTE" => {
                if !(3..=4).contains(&values.len()) {
                    return Value::Error(CalcError::Value);
                }
                let text = match values[0].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                let old = match values[1].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                let new = match values[2].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                if old.is_empty() {
                    return Value::Text(text);
                }
                let instance = if values.len() == 4 {
                    match nonnegative_integer(&values[3], false) {
                        Ok(n) if n > 0 => Some(n),
                        _ => return Value::Error(CalcError::Value),
                    }
                } else {
                    None
                };
                Value::Text(substitute_text(&text, &old, &new, instance))
            }
            "REPLACE" => {
                if values.len() != 4 {
                    return Value::Error(CalcError::Value);
                }
                let text = match values[0].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                let start = match nonnegative_integer(&values[1], false) {
                    Ok(n) if n > 0 => n,
                    _ => return Value::Error(CalcError::Value),
                };
                let count = match nonnegative_integer(&values[2], true) {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                let replacement = match values[3].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                let chars = text.chars().collect::<Vec<_>>();
                let begin = (start - 1).min(chars.len());
                let end = begin.saturating_add(count).min(chars.len());
                let mut out = chars[..begin].iter().collect::<String>();
                out.push_str(&replacement);
                out.extend(chars[end..].iter());
                Value::Text(out)
            }
            "TRIM" => {
                if values.len() != 1 {
                    return Value::Error(CalcError::Value);
                }
                match values[0].as_text() {
                    Ok(s) => Value::Text(trim_ascii_spaces(&s)),
                    Err(e) => Value::Error(e),
                }
            }
            "TEXT" => {
                if values.len() != 2 {
                    return Value::Error(CalcError::Value);
                }
                let format = match values[1].as_text() {
                    Ok(s) => s,
                    Err(e) => return Value::Error(e),
                };
                match format_value(&values[0], &format) {
                    Ok(s) => Value::Text(s),
                    Err(e) => Value::Error(e),
                }
            }
            _ => Value::Error(CalcError::Name),
        }
    }

    fn eval_find_search(&self, name: &str, values: &[Value]) -> Value {
        if !(2..=3).contains(&values.len()) {
            return Value::Error(CalcError::Value);
        }
        let needle = match values[0].as_text() {
            Ok(s) => s,
            Err(e) => return Value::Error(e),
        };
        let haystack = match values[1].as_text() {
            Ok(s) => s,
            Err(e) => return Value::Error(e),
        };
        let start = if values.len() == 3 {
            match nonnegative_integer(&values[2], false) {
                Ok(n) if n > 0 => n,
                _ => return Value::Error(CalcError::Value),
            }
        } else {
            1
        };
        let chars = haystack.chars().collect::<Vec<_>>();
        if start > chars.len() + 1 {
            return Value::Error(CalcError::Value);
        }
        if name == "FIND" {
            if needle.is_empty() {
                return Value::Number(start as f64);
            }
            let needle = needle.chars().collect::<Vec<_>>();
            let first = start - 1;
            return chars[first..]
                .windows(needle.len().max(1))
                .position(|w| w == needle.as_slice())
                .map(|offset| Value::Number((first + offset + 1) as f64))
                .unwrap_or(Value::Error(CalcError::Value));
        }
        let pattern = needle.chars().collect::<Vec<_>>();
        for begin in start.saturating_sub(1)..=chars.len() {
            for end in begin..=chars.len() {
                if wildcard_match(&pattern, &chars[begin..end], false) {
                    return Value::Number((begin + 1) as f64);
                }
            }
        }
        Value::Error(CalcError::Value)
    }

    fn eval_date_call(&mut self, name: &str, args: &[Expr], current_table: u64) -> Value {
        let values = args
            .iter()
            .map(|a| self.eval_expr(a, current_table))
            .collect::<Vec<_>>();
        if let Some(error) = values.iter().flat_map(flatten_values).find_map(value_error) {
            return Value::Error(error);
        }
        match name {
            "TODAY" if values.is_empty() => Value::DateTime(self.now_serial.floor()),
            "NOW" if values.is_empty() => Value::DateTime(self.now_serial),
            "DATE" => {
                if values.len() != 3 {
                    return Value::Error(CalcError::Value);
                }
                let year = match integer_arg(&values[0]) {
                    Ok(v) => v,
                    Err(e) => return Value::Error(e),
                };
                let month = match integer_arg(&values[1]) {
                    Ok(v) => v,
                    Err(e) => return Value::Error(e),
                };
                let day = match integer_arg(&values[2]) {
                    Ok(v) => v,
                    Err(e) => return Value::Error(e),
                };
                match date_from_components(year, month, day) {
                    Some(serial) => Value::DateTime(serial),
                    None => Value::Error(CalcError::Num),
                }
            }
            "DATEDIF" => {
                if values.len() != 3 {
                    return Value::Error(CalcError::Value);
                }
                let start = match date_parts_from_value(&values[0]) {
                    Ok(p) => p,
                    Err(e) => return Value::Error(e),
                };
                let end = match date_parts_from_value(&values[1]) {
                    Ok(p) => p,
                    Err(e) => return Value::Error(e),
                };
                if date_to_day_number(end) < date_to_day_number(start) {
                    return Value::Error(CalcError::Num);
                }
                let unit = match values[2].as_text() {
                    Ok(s) => s.to_ascii_uppercase(),
                    Err(e) => return Value::Error(e),
                };
                match datedif(start, end, &unit) {
                    Some(n) => Value::Number(n as f64),
                    None => Value::Error(CalcError::Num),
                }
            }
            "NETWORKDAYS" => {
                if !(2..=3).contains(&values.len()) {
                    return Value::Error(CalcError::Value);
                }
                let start = match date_parts_from_value(&values[0]) {
                    Ok(p) => p,
                    Err(e) => return Value::Error(e),
                };
                let end = match date_parts_from_value(&values[1]) {
                    Ok(p) => p,
                    Err(e) => return Value::Error(e),
                };
                let holidays = if values.len() == 3 {
                    let mut days = HashSet::new();
                    for value in flatten_values(&values[2]) {
                        match date_parts_from_value(value) {
                            Ok(parts) => {
                                days.insert(date_to_day_number(parts));
                            }
                            Err(_) if matches!(value, Value::Text(_) | Value::Blank) => {}
                            Err(e) => return Value::Error(e),
                        }
                    }
                    days
                } else {
                    HashSet::new()
                };
                Value::Number(networkdays(start, end, &holidays) as f64)
            }
            _ => Value::Error(CalcError::Value),
        }
    }

    fn eval_rank_eq(&mut self, args: &[Expr], current_table: u64) -> Value {
        if !(2..=3).contains(&args.len()) {
            return Value::Error(CalcError::Value);
        }
        let target = match self.eval_expr(&args[0], current_table).as_number() {
            Ok(v) => v,
            Err(e) => return Value::Error(e),
        };
        let range = self.eval_expr(&args[1], current_table);
        if let Value::Error(e) = range {
            return Value::Error(e);
        }
        let order = if args.len() == 3 {
            match self.eval_expr(&args[2], current_table).as_number() {
                Ok(v) => v,
                Err(e) => return Value::Error(e),
            }
        } else {
            0.0
        };
        let ascending = order != 0.0;
        let mut nums = Vec::new();
        for value in flatten_values(&range) {
            if let Value::Error(e) = value {
                return Value::Error(e.clone());
            }
            if let Value::Number(n) | Value::DateTime(n) = value {
                nums.push(*n);
            }
        }
        if nums.is_empty() {
            return Value::Error(CalcError::NotAvailable);
        }
        if !nums.iter().any(|n| *n == target) {
            return Value::Error(CalcError::NotAvailable);
        }
        let rank = if ascending {
            nums.iter().filter(|n| **n < target).count()
        } else {
            nums.iter().filter(|n| **n > target).count()
        } + 1;
        Value::Number(rank as f64)
    }

    fn eval_subtotal(&mut self, args: &[Expr], current_table: u64) -> Value {
        if args.len() < 2 {
            return Value::Error(CalcError::Value);
        }
        let function_num = match self.eval_expr(&args[0], current_table).as_number() {
            Ok(n) if n.fract() == 0.0 => n as i32,
            _ => return Value::Error(CalcError::Value),
        };
        let (kind, ignore_manual) = if (1..=11).contains(&function_num) {
            (function_num, false)
        } else if (101..=111).contains(&function_num) {
            (function_num - 100, true)
        } else {
            return Value::Error(CalcError::Value);
        };
        let mut data = Vec::new();
        for arg in &args[1..] {
            let addresses = match self.reference_addresses(arg, current_table) {
                Ok(Some(v)) => v,
                _ => return Value::Error(CalcError::Value),
            };
            for address in addresses {
                if self.row_is_filtered(address)
                    || (ignore_manual && self.row_is_manually_hidden(address))
                {
                    continue;
                }
                if self.cell_is_nested(address, &["SUBTOTAL"]) {
                    continue;
                }
                let value = self.eval_cell(address);
                if let Value::Error(e) = &value {
                    return Value::Error(e.clone());
                }
                data.push((Some(address), value));
            }
        }
        aggregate_values(kind, &data, None)
    }

    fn eval_aggregate(&mut self, args: &[Expr], current_table: u64) -> Value {
        if args.len() < 3 {
            return Value::Error(CalcError::Value);
        }
        let function_num = match self.eval_expr(&args[0], current_table).as_number() {
            Ok(n) if n.fract() == 0.0 => n as i32,
            _ => return Value::Error(CalcError::Value),
        };
        if !(1..=19).contains(&function_num) {
            return Value::Error(CalcError::Value);
        }
        let options = match self.eval_expr(&args[1], current_table).as_number() {
            Ok(n) if n.fract() == 0.0 && (0.0..=7.0).contains(&n) => n as u8,
            _ => return Value::Error(CalcError::Value),
        };
        let needs_k = (14..=19).contains(&function_num);
        if needs_k && args.len() != 4 {
            return Value::Error(CalcError::Value);
        }
        if !needs_k && args.len() < 3 {
            return Value::Error(CalcError::Value);
        }
        let k = if needs_k {
            match self.eval_expr(&args[3], current_table).as_number() {
                Ok(v) => Some(v),
                Err(e) => return Value::Error(e),
            }
        } else {
            None
        };
        let (ignore_nested, ignore_hidden, ignore_errors) = match options {
            0 => (true, false, false),
            1 => (true, true, false),
            2 => (true, false, true),
            3 => (true, true, true),
            4 => (false, false, false),
            5 => (false, true, false),
            6 => (false, false, true),
            7 => (false, true, true),
            _ => unreachable!(),
        };
        let mut data = Vec::new();
        for arg in &args[2..if needs_k { 3 } else { args.len() }] {
            if let Some(addresses) = match self.reference_addresses(arg, current_table) {
                Ok(v) => v,
                Err(e) => return Value::Error(e),
            } {
                for address in addresses {
                    if ignore_hidden
                        && (self.row_is_filtered(address) || self.row_is_manually_hidden(address))
                    {
                        continue;
                    }
                    if ignore_nested && self.cell_is_nested(address, &["SUBTOTAL", "AGGREGATE"]) {
                        continue;
                    }
                    let value = self.eval_cell(address);
                    if matches!(value, Value::Error(_)) && ignore_errors {
                        continue;
                    }
                    if let Value::Error(e) = &value {
                        return Value::Error(e.clone());
                    }
                    data.push((Some(address), value));
                }
            } else {
                let value = self.eval_expr(arg, current_table);
                for item in flatten_values(&value) {
                    if matches!(item, Value::Error(_)) && ignore_errors {
                        continue;
                    }
                    if let Value::Error(e) = item {
                        return Value::Error(e.clone());
                    }
                    data.push((None, item.clone()));
                }
            }
        }
        aggregate_values(function_num, &data, k)
    }

    fn reference_addresses(
        &self,
        expr: &Expr,
        current_table: u64,
    ) -> Result<Option<Vec<CellAddress>>, CalcError> {
        let (a, b) = match expr {
            Expr::Ref(a) => (a, a),
            Expr::Range(a, b) => (a, b),
            _ => return Ok(None),
        };
        let table_id = a.table_id.or(b.table_id).unwrap_or(current_table);
        if a.table_id.is_some_and(|id| id != table_id)
            || b.table_id.is_some_and(|id| id != table_id)
        {
            return Err(CalcError::Ref);
        }
        let table = self.tables.get(&table_id).ok_or(CalcError::Ref)?;
        let max_row = table.cells.len();
        let max_column = table.cells.iter().map(Vec::len).max().unwrap_or(0);
        if a.row >= max_row || b.row >= max_row || a.column >= max_column || b.column >= max_column
        {
            return Err(CalcError::Ref);
        }
        let mut out = Vec::new();
        for row in a.row.min(b.row)..=a.row.max(b.row) {
            for column in a.column.min(b.column)..=a.column.max(b.column) {
                if table.cells.get(row).and_then(|r| r.get(column)).is_none() {
                    return Err(CalcError::Ref);
                }
                out.push(CellAddress {
                    table_id,
                    coord: CellCoord { row, column },
                });
            }
        }
        Ok(Some(out))
    }

    fn row_is_manually_hidden(&self, address: CellAddress) -> bool {
        self.tables
            .get(&address.table_id)
            .is_some_and(|table| table.manually_hidden_rows.contains(&address.coord.row))
    }

    fn row_is_filtered(&self, address: CellAddress) -> bool {
        self.tables
            .get(&address.table_id)
            .is_some_and(|table| table.filtered_out_rows.contains(&address.coord.row))
    }

    fn cell_is_nested(&self, address: CellAddress, names: &[&str]) -> bool {
        let Some(table) = self.tables.get(&address.table_id) else {
            return false;
        };
        if !table.formula_cells.contains(&address.coord) {
            return false;
        }
        let Some(raw) = table
            .cells
            .get(address.coord.row)
            .and_then(|r| r.get(address.coord.column))
        else {
            return false;
        };
        match parse_formula(raw) {
            Ok(Expr::Call(name, _)) => names.iter().any(|n| name.eq_ignore_ascii_case(n)),
            _ => false,
        }
    }

    fn eval_eager_call(&mut self, name: &str, args: &[Expr], current_table: u64) -> Value {
        let mut values = Vec::new();
        for arg in args {
            values.push(self.eval_expr(arg, current_table));
        }
        if let Some(error) = values.iter().find_map(|v| {
            if let Value::Error(e) = v {
                Some(e.clone())
            } else {
                None
            }
        }) {
            return Value::Error(error);
        }
        let flat = values.iter().flat_map(flatten_values).collect::<Vec<_>>();
        if let Some(error) = flat.iter().find_map(|v| {
            if let Value::Error(e) = v {
                Some(e.clone())
            } else {
                None
            }
        }) {
            return Value::Error(error);
        }
        let nums: Vec<f64> = flat
            .iter()
            .filter_map(|v| match v {
                Value::Number(n) | Value::DateTime(n) => Some(*n),
                _ => None,
            })
            .collect();
        match name {
            "SUM" => Value::Number(nums.iter().sum()),
            "AVERAGE" => {
                if nums.is_empty() {
                    Value::Error(CalcError::Div0)
                } else {
                    Value::Number(nums.iter().sum::<f64>() / nums.len() as f64)
                }
            }
            "PRODUCT" => {
                if nums.is_empty() {
                    Value::Number(0.0)
                } else {
                    Value::Number(nums.iter().product())
                }
            }
            "COUNT" => Value::Number(nums.len() as f64),
            "COUNTA" => {
                Value::Number(flat.iter().filter(|v| !matches!(v, Value::Blank)).count() as f64)
            }
            "ABS" if values.len() == 1 => number_call(&values, |n| n.abs()),
            "ROUND" if values.len() == 2 => {
                round_call(&values, |n, p| (n * 10f64.powi(p)).round() / 10f64.powi(p))
            }
            "ROUNDDOWN" if values.len() == 2 => {
                round_call(&values, |n, p| (n * 10f64.powi(p)).trunc() / 10f64.powi(p))
            }
            "ROUNDUP" if values.len() == 2 => round_call(&values, |n, p| {
                let factor = 10f64.powi(p);
                let scaled = n * factor;
                (if scaled >= 0.0 {
                    scaled.ceil()
                } else {
                    scaled.floor()
                }) / factor
            }),
            "MOD" if values.len() == 2 => {
                let a = match values[0].as_number() {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                let b = match values[1].as_number() {
                    Ok(n) => n,
                    Err(e) => return Value::Error(e),
                };
                if b == 0.0 {
                    Value::Error(CalcError::Div0)
                } else {
                    Value::Number(a - b * (a / b).floor())
                }
            }
            _ => Value::Error(CalcError::Name),
        }
    }
}

fn flatten_values(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(array) => array.values.iter().flat_map(flatten_values).collect(),
        _ => vec![value],
    }
}

fn aggregate_values(
    function_num: i32,
    data: &[(Option<CellAddress>, Value)],
    k: Option<f64>,
) -> Value {
    let values = data.iter().map(|(_, value)| value).collect::<Vec<_>>();
    if let Some(Value::Error(error)) = values.iter().find(|value| matches!(value, Value::Error(_)))
    {
        return Value::Error(error.clone());
    }
    let mut numbers = Vec::new();
    for value in &values {
        match value {
            Value::Number(n) | Value::DateTime(n) if n.is_finite() => numbers.push(*n),
            Value::Number(_) | Value::DateTime(_) => return Value::Error(CalcError::Num),
            _ => {}
        }
    }
    match function_num {
        1 => {
            if numbers.is_empty() {
                Value::Error(CalcError::Div0)
            } else {
                Value::Number(numbers.iter().sum::<f64>() / numbers.len() as f64)
            }
        }
        2 => Value::Number(numbers.len() as f64),
        3 => Value::Number(values.iter().filter(|v| !matches!(v, Value::Blank)).count() as f64),
        4 => Value::Number(numbers.iter().copied().reduce(f64::max).unwrap_or(0.0)),
        5 => Value::Number(numbers.iter().copied().reduce(f64::min).unwrap_or(0.0)),
        6 => {
            if numbers.is_empty() {
                Value::Number(0.0)
            } else {
                Value::Number(numbers.iter().product())
            }
        }
        7 => variance(&numbers, true)
            .map(|v| Value::Number(v.sqrt()))
            .unwrap_or(Value::Error(CalcError::Div0)),
        8 => variance(&numbers, false)
            .map(|v| Value::Number(v.sqrt()))
            .unwrap_or(Value::Error(CalcError::Div0)),
        9 => Value::Number(numbers.iter().sum()),
        10 => variance(&numbers, true)
            .map(Value::Number)
            .unwrap_or(Value::Error(CalcError::Div0)),
        11 => variance(&numbers, false)
            .map(Value::Number)
            .unwrap_or(Value::Error(CalcError::Div0)),
        12 => median(&mut numbers)
            .map(Value::Number)
            .unwrap_or(Value::Error(CalcError::Num)),
        13 => mode_single(&numbers)
            .map(Value::Number)
            .unwrap_or(Value::Error(CalcError::NotAvailable)),
        14 => order_statistic(&mut numbers, k, true),
        15 => order_statistic(&mut numbers, k, false),
        16 => percentile_inc(&mut numbers, k),
        17 => match k {
            Some(q) if q.fract() == 0.0 && (0.0..=4.0).contains(&q) => {
                percentile_inc(&mut numbers, Some(q / 4.0))
            }
            _ => Value::Error(CalcError::Num),
        },
        18 => percentile_exc(&mut numbers, k),
        19 => match k {
            Some(q) if q.fract() == 0.0 && (1.0..=3.0).contains(&q) => {
                percentile_exc(&mut numbers, Some(q / 4.0))
            }
            _ => Value::Error(CalcError::Num),
        },
        _ => Value::Error(CalcError::Value),
    }
}

fn variance(values: &[f64], sample: bool) -> Option<f64> {
    let n = values.len();
    let divisor = if sample { n.checked_sub(1)? } else { n };
    if divisor == 0 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    Some(values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / divisor as f64)
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    })
}

fn mode_single(values: &[f64]) -> Option<f64> {
    let mut sorted = values.to_vec();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    let mut best = None;
    let mut best_count = 1usize;
    let mut index = 0;
    while index < sorted.len() {
        let mut end = index + 1;
        while end < sorted.len() && sorted[end] == sorted[index] {
            end += 1;
        }
        let count = end - index;
        if count > best_count {
            best = Some(sorted[index]);
            best_count = count;
        }
        index = end;
    }
    best
}

fn order_statistic(values: &mut [f64], k: Option<f64>, largest: bool) -> Value {
    let Some(k) = k else {
        return Value::Error(CalcError::Value);
    };
    if !k.is_finite() || k.fract() != 0.0 || k < 1.0 || k > values.len() as f64 {
        return Value::Error(CalcError::Num);
    }
    values.sort_by(f64::total_cmp);
    let index = k as usize - 1;
    Value::Number(if largest {
        values[values.len() - 1 - index]
    } else {
        values[index]
    })
}

fn percentile_inc(values: &mut [f64], k: Option<f64>) -> Value {
    let Some(k) = k else {
        return Value::Error(CalcError::Value);
    };
    if values.is_empty() || !k.is_finite() || !(0.0..=1.0).contains(&k) {
        return Value::Error(CalcError::Num);
    }
    values.sort_by(f64::total_cmp);
    let index = (values.len() - 1) as f64 * k;
    interpolate_sorted(values, index)
}

fn percentile_exc(values: &mut [f64], k: Option<f64>) -> Value {
    let Some(k) = k else {
        return Value::Error(CalcError::Value);
    };
    if values.is_empty() || !k.is_finite() || !(0.0..1.0).contains(&k) {
        return Value::Error(CalcError::Num);
    }
    values.sort_by(f64::total_cmp);
    let rank = (values.len() + 1) as f64 * k;
    if !(1.0..=values.len() as f64).contains(&rank) {
        return Value::Error(CalcError::Num);
    }
    interpolate_sorted(values, rank - 1.0)
}

fn interpolate_sorted(values: &[f64], index: f64) -> Value {
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    if lower >= values.len() || upper >= values.len() {
        return Value::Error(CalcError::Num);
    }
    let ratio = index - lower as f64;
    Value::Number(values[lower] + (values[upper] - values[lower]) * ratio)
}

fn value_error(value: &Value) -> Option<CalcError> {
    match value {
        Value::Error(e) => Some(e.clone()),
        _ => None,
    }
}

fn optional_char_count(values: &[Value], index: usize) -> Result<usize, CalcError> {
    match values.get(index) {
        Some(v) => nonnegative_integer(v, true),
        None => Ok(1),
    }
}

fn nonnegative_integer(value: &Value, allow_zero: bool) -> Result<usize, CalcError> {
    let n = value.as_number()?;
    if !n.is_finite()
        || n.fract() != 0.0
        || n < 0.0
        || (!allow_zero && n == 0.0)
        || n > usize::MAX as f64
    {
        return Err(CalcError::Value);
    }
    Ok(n as usize)
}

fn criteria_matches(value: &Value, criterion: &Value) -> bool {
    let criterion_text = match criterion {
        Value::Text(s) => s.as_str().to_owned(),
        Value::Blank => String::new(),
        Value::DateTime(n) => format_number(*n),
        v => v.display_text(),
    };
    let (op, needle) = if let Some(rest) = criterion_text.strip_prefix("<=") {
        ("<=", rest)
    } else if let Some(rest) = criterion_text.strip_prefix(">=") {
        (">=", rest)
    } else if let Some(rest) = criterion_text.strip_prefix("<>") {
        ("<>", rest)
    } else if let Some(rest) = criterion_text.strip_prefix('<') {
        ("<", rest)
    } else if let Some(rest) = criterion_text.strip_prefix('>') {
        (">", rest)
    } else if let Some(rest) = criterion_text.strip_prefix('=') {
        ("=", rest)
    } else {
        ("=", criterion_text.as_str())
    };

    if op == "=" || op == "<>" {
        let equal = if let Some(target) = criterion_number(needle) {
            matches!(value, Value::Number(n) | Value::DateTime(n) if *n == target)
        } else {
            let text = match value {
                Value::Text(s) => s.as_str(),
                Value::Blank => "",
                Value::Bool(v) => {
                    if *v {
                        "TRUE"
                    } else {
                        "FALSE"
                    }
                }
                Value::Number(_) | Value::DateTime(_) => return op == "<>",
                Value::Error(_) | Value::Array(_) => return false,
            };
            let pattern = needle.chars().collect::<Vec<_>>();
            if pattern.iter().any(|c| matches!(c, '*' | '?' | '~')) {
                wildcard_match(&pattern, &text.chars().collect::<Vec<_>>(), false)
            } else {
                text.eq_ignore_ascii_case(needle)
            }
        };
        return if op == "<>" { !equal } else { equal };
    }

    let ordering = if let Some(target) = criterion_number(needle) {
        match value {
            Value::Number(n) | Value::DateTime(n) => n.partial_cmp(&target),
            Value::Blank => 0.0f64.partial_cmp(&target),
            _ => None,
        }
    } else {
        let text = match value {
            Value::Text(s) => s.as_str(),
            Value::Blank => "",
            _ => return false,
        };
        Some(text.to_lowercase().cmp(&needle.to_lowercase()))
    };
    match (op, ordering) {
        ("<", Some(o)) => o.is_lt(),
        ("<=", Some(o)) => !o.is_gt(),
        (">", Some(o)) => o.is_gt(),
        (">=", Some(o)) => !o.is_lt(),
        _ => false,
    }
}

fn criterion_number(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .or_else(|| parse_date_literal(text))
}

/// Spreadsheet-style wildcard match. `~` escapes the next wildcard character.
fn wildcard_match(pattern: &[char], text: &[char], case_sensitive: bool) -> bool {
    #[derive(Clone, Copy)]
    enum P {
        Any,
        One,
        Literal(char),
    }
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < pattern.len() {
        match pattern[i] {
            '~' if i + 1 < pattern.len() => {
                i += 1;
                tokens.push(P::Literal(pattern[i]));
            }
            '*' => tokens.push(P::Any),
            '?' => tokens.push(P::One),
            c => tokens.push(P::Literal(c)),
        }
        i += 1;
    }
    let equal = |a: char, b: char| {
        if case_sensitive {
            a == b
        } else {
            a.to_lowercase().to_string() == b.to_lowercase().to_string()
        }
    };
    let mut dp = vec![vec![false; text.len() + 1]; tokens.len() + 1];
    dp[0][0] = true;
    for p in 1..=tokens.len() {
        if matches!(tokens[p - 1], P::Any) {
            dp[p][0] = dp[p - 1][0];
        }
        for t in 1..=text.len() {
            dp[p][t] = match tokens[p - 1] {
                P::Any => dp[p - 1][t] || dp[p][t - 1],
                P::One => dp[p - 1][t - 1],
                P::Literal(c) => dp[p - 1][t - 1] && equal(c, text[t - 1]),
            };
        }
    }
    dp[tokens.len()][text.len()]
}

fn substitute_text(text: &str, old: &str, new: &str, instance: Option<usize>) -> String {
    let hits = text.match_indices(old).map(|(i, _)| i).collect::<Vec<_>>();
    if let Some(n) = instance {
        let Some(&start) = hits.get(n - 1) else {
            return text.to_owned();
        };
        let mut out = String::with_capacity(text.len() + new.len());
        out.push_str(&text[..start]);
        out.push_str(new);
        out.push_str(&text[start + old.len()..]);
        return out;
    }
    let mut out = String::new();
    let mut at = 0;
    for start in hits {
        if start < at {
            continue;
        }
        out.push_str(&text[at..start]);
        out.push_str(new);
        at = start + old.len();
    }
    out.push_str(&text[at..]);
    out
}

fn trim_ascii_spaces(text: &str) -> String {
    let mut out = String::new();
    let mut was_space = false;
    for ch in text.trim_matches(' ').chars() {
        if ch == ' ' {
            if !was_space {
                out.push(ch);
            }
            was_space = true;
        } else {
            out.push(ch);
            was_space = false;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DateParts {
    year: i32,
    month: u32,
    day: u32,
}

fn integer_arg(value: &Value) -> Result<i32, CalcError> {
    let number = value.as_number()?;
    if !number.is_finite() || number.trunc() < i32::MIN as f64 || number.trunc() > i32::MAX as f64 {
        return Err(CalcError::Num);
    }
    Ok(number.trunc() as i32)
}

fn date_from_components(year: i32, month: i32, day: i32) -> Option<f64> {
    let month_index = month.checked_sub(1)?;
    let normalized_year = year.checked_add(month_index.div_euclid(12))?;
    let normalized_month = month_index.rem_euclid(12) as u32 + 1;
    if !(1900..=9999).contains(&normalized_year) {
        return None;
    }
    // The fictitious leap day is deliberately rejected.
    if normalized_year == 1900 && normalized_month == 2 && day == 29 {
        return None;
    }
    let first = days_from_civil(normalized_year, normalized_month, 1);
    let days = first.checked_add(day as i64 - 1)?;
    let serial = days - days_from_civil(1899, 12, 31);
    Some((serial + i64::from(days >= days_from_civil(1900, 3, 1))) as f64)
}

fn parse_date_literal(text: &str) -> Option<f64> {
    if text.len() != 10 {
        return None;
    }
    let bytes = text.as_bytes();
    let sep = *bytes.get(4)?;
    if (sep != b'-' && sep != b'/')
        || bytes.get(7).copied()? != sep
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let year = text[0..4].parse::<i32>().ok()?;
    let month = text[5..7].parse::<i32>().ok()?;
    let day = text[8..10].parse::<i32>().ok()?;
    let serial = date_from_components(year, month, day)?;
    let parts = date_parts_from_serial(serial)?;
    (parts.year == year && parts.month == month as u32 && parts.day == day as u32).then_some(serial)
}

fn date_parts_from_value(value: &Value) -> Result<DateParts, CalcError> {
    let serial = match value {
        Value::DateTime(n) | Value::Number(n) => *n,
        Value::Text(s) => parse_date_literal(s).ok_or(CalcError::Value)?,
        _ => return Err(CalcError::Value),
    };
    date_parts_from_serial(serial).ok_or(CalcError::Num)
}

fn date_parts_from_serial(serial: f64) -> Option<DateParts> {
    if !serial.is_finite() || serial < 0.0 || serial > 2_958_465.999_999 {
        return None;
    }
    let whole = serial.floor() as i64;
    if whole == 60 {
        return None;
    }
    let adjusted = if whole > 60 { whole - 1 } else { whole };
    let day_number = days_from_civil(1899, 12, 31).checked_add(adjusted)?;
    let (year, month, day) = civil_from_days(day_number);
    if !(1900..=9999).contains(&year) {
        return None;
    }
    Some(DateParts { year, month, day })
}

fn date_to_day_number(parts: DateParts) -> i64 {
    days_from_civil(parts.year, parts.month, parts.day)
}

fn days_from_civil(mut year: i32, month: u32, day: u32) -> i64 {
    year -= i32::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = month as i32 + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era as i64 * 146_097 + day_of_era as i64 - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era as i32 + era as i32 * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = (month_prime + if month_prime < 10 { 3 } else { -9 }) as u32;
    year += i32::from(month <= 2);
    (year, month, day)
}

fn datedif(start: DateParts, end: DateParts, unit: &str) -> Option<i64> {
    let years = end.year - start.year;
    let months = years * 12 + end.month as i32 - start.month as i32;
    let anniversary_passed = (end.month, end.day) >= (start.month, start.day);
    match unit {
        "D" => Some(date_to_day_number(end) - date_to_day_number(start)),
        "Y" => Some((years - i32::from(!anniversary_passed)) as i64),
        "M" => Some((months - i32::from(end.day < start.day)) as i64),
        "YM" => Some((months - i32::from(end.day < start.day)).rem_euclid(12) as i64),
        "YD" => {
            let sy = DateParts {
                year: start.year,
                ..end
            };
            let mut finish = date_to_day_number(sy) - date_to_day_number(start);
            if finish < 0 {
                let next_year = DateParts {
                    year: start.year + 1,
                    ..end
                };
                finish = date_to_day_number(next_year) - date_to_day_number(start);
            }
            Some(finish)
        }
        "MD" => {
            if end.day >= start.day {
                Some((end.day - start.day) as i64)
            } else {
                let (previous_year, previous_month) = if end.month == 1 {
                    (end.year - 1, 12)
                } else {
                    (end.year, end.month - 1)
                };
                let (next_year, next_month) = if previous_month == 12 {
                    (previous_year + 1, 1)
                } else {
                    (previous_year, previous_month + 1)
                };
                let days_in_previous_month = days_from_civil(next_year, next_month, 1)
                    - days_from_civil(previous_year, previous_month, 1);
                Some(days_in_previous_month - start.day as i64 + end.day as i64)
            }
        }
        _ => None,
    }
}

fn networkdays(start: DateParts, end: DateParts, holidays: &HashSet<i64>) -> i64 {
    let first = date_to_day_number(start);
    let last = date_to_day_number(end);
    if first > last {
        return -networkdays(end, start, holidays);
    }
    let days = last - first + 1;
    let mut count = days / 7 * 5;
    for offset in 0..days % 7 {
        let weekday = (first + offset + 3).rem_euclid(7);
        if weekday < 5 {
            count += 1;
        }
    }
    count -= holidays
        .iter()
        .filter(|day| **day >= first && **day <= last && (**day + 3).rem_euclid(7) < 5)
        .count() as i64;
    count
}

fn local_now_serial() -> f64 {
    let (year, month, day, hour, minute, second, millis) = local_clock_parts();
    date_from_components(year, month as i32, day as i32).unwrap_or(0.0)
        + (hour as f64 * 3_600.0 + minute as f64 * 60.0 + second as f64 + millis as f64 / 1_000.0)
            / 86_400.0
}

#[cfg(windows)]
fn local_clock_parts() -> (i32, u32, u32, u32, u32, u32, u32) {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    unsafe extern "system" {
        fn GetLocalTime(time: *mut SystemTime);
    }
    let mut time = SystemTime {
        year: 0,
        month: 0,
        day_of_week: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        milliseconds: 0,
    };
    // GetLocalTime writes the complete SYSTEMTIME structure synchronously.
    unsafe {
        GetLocalTime(&mut time);
    }
    (
        time.year as i32,
        time.month as u32,
        time.day as u32,
        time.hour as u32,
        time.minute as u32,
        time.second as u32,
        time.milliseconds as u32,
    )
}

#[cfg(unix)]
fn local_clock_parts() -> (i32, u32, u32, u32, u32, u32, u32) {
    use std::ffi::{c_char, c_int, c_long};
    #[repr(C)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
        gmtoff: c_long,
        zone: *const c_char,
    }
    unsafe extern "C" {
        fn time(timer: *mut c_long) -> c_long;
        fn localtime_r(timer: *const c_long, result: *mut Tm) -> *mut Tm;
    }
    let mut timestamp: c_long = 0;
    unsafe {
        time(&mut timestamp);
    }
    let mut tm = Tm {
        sec: 0,
        min: 0,
        hour: 0,
        mday: 0,
        mon: 0,
        year: 0,
        wday: 0,
        yday: 0,
        isdst: 0,
        gmtoff: 0,
        zone: std::ptr::null(),
    };
    let result = unsafe { localtime_r(&timestamp, &mut tm) };
    if result.is_null() {
        return utc_clock_parts();
    }
    (
        tm.year + 1900,
        (tm.mon + 1) as u32,
        tm.mday as u32,
        tm.hour as u32,
        tm.min as u32,
        tm.sec as u32,
        0,
    )
}

#[cfg(not(any(windows, unix)))]
fn local_clock_parts() -> (i32, u32, u32, u32, u32, u32, u32) {
    utc_clock_parts()
}

#[cfg(not(windows))]
fn utc_clock_parts() -> (i32, u32, u32, u32, u32, u32, u32) {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = duration.as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let within_day = seconds.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        within_day / 3_600,
        (within_day / 60) % 60,
        within_day % 60,
        duration.subsec_millis(),
    )
}

fn format_datetime_default(serial: f64) -> String {
    let Some(parts) = date_parts_from_serial(serial) else {
        return "#NUM!".to_owned();
    };
    let mut out = format!("{:04}-{:02}-{:02}", parts.year, parts.month, parts.day);
    let fraction = serial - serial.floor();
    if fraction > 0.0 {
        let total_seconds = (fraction * 86_400.0).round() as u32;
        let h = (total_seconds / 3_600) % 24;
        let m = (total_seconds / 60) % 60;
        let s = total_seconds % 60;
        out.push_str(&format!(" {h:02}:{m:02}:{s:02}"));
    }
    out
}

fn format_value(value: &Value, format: &str) -> Result<String, CalcError> {
    let mut unquoted = String::new();
    let mut quoted = false;
    for c in format.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            unquoted.push(c);
        }
    }
    if quoted {
        return Err(CalcError::Value);
    }
    let is_date_format = unquoted.chars().any(|c| c.is_ascii_alphabetic());
    if is_date_format {
        let serial = value.as_number()?;
        return format_date_pattern(serial, format);
    }
    let number = value.as_number()?;
    format_number_pattern(number, format)
}

fn format_number_pattern(number: f64, format: &str) -> Result<String, CalcError> {
    if !number.is_finite() {
        return Err(CalcError::Num);
    }
    let mut literal_prefix = String::new();
    let mut literal_suffix = String::new();
    let mut mask = String::new();
    let mut in_quotes = false;
    let mut seen_mask = false;
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if in_quotes {
            if seen_mask {
                literal_suffix.push(c);
            } else {
                literal_prefix.push(c);
            }
            continue;
        }
        if matches!(c, '0' | '#' | '.' | ',' | '%') {
            mask.push(c);
            seen_mask = true;
        } else if !seen_mask {
            literal_prefix.push(c);
        } else {
            literal_suffix.push(c);
        }
    }
    if in_quotes || !mask.chars().any(|c| matches!(c, '0' | '#')) {
        return Err(CalcError::Value);
    }
    let percent = mask.matches('%').count();
    let scaled = number * 100f64.powi(percent as i32);
    let mask = mask.replace('%', "");
    let parts = mask.split_once('.');
    let integer_mask = parts.map(|p| p.0).unwrap_or(&mask);
    let decimal_mask = parts.map(|p| p.1).unwrap_or("");
    let decimals = decimal_mask
        .chars()
        .filter(|c| matches!(c, '0' | '#'))
        .count();
    let min_decimals = decimal_mask.chars().filter(|c| *c == '0').count();
    let raw = format!("{:.*}", decimals, scaled.abs());
    let (integer, decimal) = raw.split_once('.').unwrap_or((&raw, ""));
    let mut int = integer.to_owned();
    let min_integer = integer_mask.chars().filter(|c| *c == '0').count();
    if int.len() < min_integer {
        int = format!("{}{}", "0".repeat(min_integer - int.len()), int);
    }
    if integer_mask.contains(',') {
        int = group_thousands(&int);
    }
    let mut out = String::new();
    if number < 0.0 {
        out.push('-');
    }
    out.push_str(&literal_prefix);
    out.push_str(&int);
    let mut decimals_out = decimal.to_owned();
    while decimals_out.len() > min_decimals && decimals_out.ends_with('0') {
        decimals_out.pop();
    }
    if !decimals_out.is_empty() || min_decimals > 0 {
        out.push('.');
        out.push_str(&decimals_out);
    }
    if percent > 0 {
        out.push_str(&"%".repeat(percent));
    }
    out.push_str(&literal_suffix);
    Ok(out)
}

fn group_thousands(input: &str) -> String {
    let mut out = String::new();
    for (i, ch) in input.chars().enumerate() {
        if i > 0 && (input.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn format_date_pattern(serial: f64, pattern: &str) -> Result<String, CalcError> {
    let parts = date_parts_from_serial(serial).ok_or(CalcError::Num)?;
    let fraction = serial - serial.floor();
    let seconds = (fraction * 86_400.0).round() as u32;
    let hour = (seconds / 3_600) % 24;
    let minute = (seconds / 60) % 60;
    let second = seconds % 60;
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut i = 0;
    let mut quoted = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quoted = !quoted;
            i += 1;
            continue;
        }
        if quoted {
            out.push(c);
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].to_ascii_lowercase() == c.to_ascii_lowercase() {
                i += 1;
            }
            let token = chars[start..i].iter().collect::<String>();
            let lower = token.to_ascii_lowercase();
            let before_has_hour = chars[..start]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase()
                .contains('h');
            let after_has_seconds = chars[i..]
                .iter()
                .collect::<String>()
                .to_ascii_lowercase()
                .contains('s');
            let time_minute =
                (start > 0 && chars[start - 1] == ':') || (i < chars.len() && chars[i] == ':');
            let rendered = match lower.as_str() {
                "yyyy" => format!("{:04}", parts.year),
                "yy" => format!("{:02}", parts.year.rem_euclid(100)),
                "m" | "mm" if time_minute && (before_has_hour || after_has_seconds) => {
                    if lower == "mm" {
                        format!("{minute:02}")
                    } else {
                        minute.to_string()
                    }
                }
                "m" => parts.month.to_string(),
                "mm" => format!("{:02}", parts.month),
                "d" => parts.day.to_string(),
                "dd" => format!("{:02}", parts.day),
                "h" => hour.to_string(),
                "hh" => format!("{hour:02}"),
                "s" => second.to_string(),
                "ss" => format!("{second:02}"),
                _ => return Err(CalcError::Value),
            };
            out.push_str(&rendered);
        } else {
            out.push(c);
            i += 1;
        }
    }
    if quoted {
        return Err(CalcError::Value);
    }
    Ok(out)
}

fn parse_literal(raw: &str) -> Value {
    if raw.is_empty() {
        return Value::Blank;
    }
    if let Some(text) = raw.strip_prefix("'=") {
        return Value::Text(format!("={text}"));
    }
    if raw == "TRUE" || raw.eq_ignore_ascii_case("TRUE") {
        return Value::Bool(true);
    }
    if raw.eq_ignore_ascii_case("FALSE") {
        return Value::Bool(false);
    }
    if let Some(serial) = parse_date_literal(raw) {
        return Value::DateTime(serial);
    }
    let trimmed = raw.trim();
    let leading_zero = trimmed.len() > 1
        && trimmed.starts_with('0')
        && trimmed.as_bytes().get(1).is_some_and(u8::is_ascii_digit);
    if !leading_zero {
        if let Ok(n) = trimmed.parse::<f64>() {
            return Value::Number(n);
        }
    }
    Value::Text(raw.to_owned())
}

fn number_call(values: &[Value], f: impl FnOnce(f64) -> f64) -> Value {
    if values.len() != 1 {
        return Value::Error(CalcError::Value);
    }
    match values[0].as_number() {
        Ok(n) => Value::Number(f(n)),
        Err(e) => Value::Error(e),
    }
}

fn round_call(values: &[Value], f: impl FnOnce(f64, i32) -> f64) -> Value {
    if values.len() != 2 {
        return Value::Error(CalcError::Value);
    }
    let n = match values[0].as_number() {
        Ok(n) => n,
        Err(e) => return Value::Error(e),
    };
    let p = match values[1].as_number() {
        Ok(n) if n.abs() <= 308.0 => n as i32,
        _ => return Value::Error(CalcError::Num),
    };
    Value::Number(f(n, p))
}

fn apply_unary(op: UnaryOp, value: Value) -> Value {
    match value {
        Value::Array(array) => {
            let values = array
                .values
                .into_iter()
                .map(|v| apply_unary(op, v))
                .collect();
            Value::Array(ArrayValue {
                rows: array.rows,
                columns: array.columns,
                values,
            })
        }
        value => match op {
            UnaryOp::Plus => value
                .as_number()
                .map(Value::Number)
                .unwrap_or_else(Value::Error),
            UnaryOp::Minus => value
                .as_number()
                .map(|n| Value::Number(-n))
                .unwrap_or_else(Value::Error),
            UnaryOp::Percent => value
                .as_number()
                .map(|n| Value::Number(n / 100.0))
                .unwrap_or_else(Value::Error),
        },
    }
}

fn eval_binary(op: BinaryOp, l: Value, r: Value) -> Value {
    use BinaryOp::*;
    match (&l, &r) {
        (Value::Array(a), Value::Array(b)) => {
            if a.rows != b.rows || a.columns != b.columns {
                return Value::Error(CalcError::Value);
            }
            let values = a
                .values
                .iter()
                .zip(&b.values)
                .map(|(x, y)| eval_binary(op, x.clone(), y.clone()))
                .collect();
            return Value::Array(ArrayValue {
                rows: a.rows,
                columns: a.columns,
                values,
            });
        }
        (Value::Array(a), scalar) => {
            let values = a
                .values
                .iter()
                .map(|x| eval_binary(op, x.clone(), scalar.clone()))
                .collect();
            return Value::Array(ArrayValue {
                rows: a.rows,
                columns: a.columns,
                values,
            });
        }
        (scalar, Value::Array(b)) => {
            let values = b
                .values
                .iter()
                .map(|y| eval_binary(op, scalar.clone(), y.clone()))
                .collect();
            return Value::Array(ArrayValue {
                rows: b.rows,
                columns: b.columns,
                values,
            });
        }
        _ => {}
    }
    if let Value::Error(e) = l {
        return Value::Error(e);
    }
    if let Value::Error(e) = r {
        return Value::Error(e);
    }
    match op {
        Add | Sub | Mul | Div | Pow => {
            let a = match l.as_number() {
                Ok(n) => n,
                Err(e) => return Value::Error(e),
            };
            let b = match r.as_number() {
                Ok(n) => n,
                Err(e) => return Value::Error(e),
            };
            let result = match op {
                Add => a + b,
                Sub => a - b,
                Mul => a * b,
                Div if b == 0.0 => return Value::Error(CalcError::Div0),
                Div => a / b,
                Pow => a.powf(b),
                _ => unreachable!(),
            };
            if result.is_finite() {
                Value::Number(result)
            } else {
                Value::Error(CalcError::Num)
            }
        }
        Concat => match (l.as_text(), r.as_text()) {
            (Ok(a), Ok(b)) => Value::Text(a + &b),
            (Err(e), _) | (_, Err(e)) => Value::Error(e),
        },
        Eq | Ne | Lt | Le | Gt | Ge => {
            let ordering = match (&l, &r) {
                (Value::Number(a) | Value::DateTime(a), Value::Number(b) | Value::DateTime(b)) => {
                    a.partial_cmp(b)
                }
                (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
                _ => l
                    .as_text()
                    .ok()
                    .zip(r.as_text().ok())
                    .map(|(a, b)| a.to_lowercase().cmp(&b.to_lowercase())),
            };
            let Some(ord) = ordering else {
                return Value::Error(CalcError::Value);
            };
            Value::Bool(match op {
                Eq => ord.is_eq(),
                Ne => !ord.is_eq(),
                Lt => ord.is_lt(),
                Le => !ord.is_gt(),
                Gt => ord.is_gt(),
                Ge => !ord.is_lt(),
                _ => false,
            })
        }
    }
}
