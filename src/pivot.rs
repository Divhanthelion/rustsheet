//! Pivot tables: summarize a range by the values in some of its columns.
//!
//! A [`PivotTable`] names its source (a sheet and a range whose first row
//! holds the field names) and puts fields in rows, columns, values and
//! report filters. [`compute`] turns the source's records into the table's
//! cells. The app writes those cells as plain values, so the file shows the
//! same anywhere, and keeps the definition to refresh them.

use crate::cell::{CellCoord, CellRange, LineEdit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aggregate {
    #[default]
    Sum,
    Count,
    Average,
    Min,
    Max,
}

impl Aggregate {
    pub const ALL: [Aggregate; 5] = [
        Aggregate::Sum,
        Aggregate::Count,
        Aggregate::Average,
        Aggregate::Min,
        Aggregate::Max,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Aggregate::Sum => "Sum",
            Aggregate::Count => "Count",
            Aggregate::Average => "Average",
            Aggregate::Min => "Min",
            Aggregate::Max => "Max",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PivotValue {
    /// Source column, counted from the source range's first column
    pub field: usize,
    pub aggregate: Aggregate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PivotTable {
    pub name: String,
    /// The sheet holding the source data, by name
    pub source_sheet: String,
    /// The source range; its first row names the fields
    pub source: CellRange,
    /// Top-left cell of the table (filters first) on its own sheet
    pub anchor: CellCoord,
    pub rows: Vec<usize>,
    pub columns: Vec<usize>,
    pub values: Vec<PivotValue>,
    /// Report filters, listed above the table
    pub filters: Vec<usize>,
    /// Items left out, by field, as shown
    #[serde(default)]
    pub hidden: BTreeMap<usize, BTreeSet<String>>,
    /// The cells written last time, cleared before the next refresh
    #[serde(default)]
    pub output: Option<CellRange>,
}

impl PivotTable {
    pub fn new(name: String, source_sheet: String, source: CellRange, anchor: CellCoord) -> Self {
        Self {
            name,
            source_sheet,
            source,
            anchor,
            rows: Vec::new(),
            columns: Vec::new(),
            values: Vec::new(),
            filters: Vec::new(),
            hidden: BTreeMap::new(),
            output: None,
        }
    }

    /// Whether `coord` (on the table's sheet) is in the table.
    pub fn contains(&self, coord: CellCoord) -> bool {
        let area = self.output.unwrap_or(CellRange::single(self.anchor));
        (area.start.row..=area.end.row).contains(&coord.row)
            && (area.start.col..=area.end.col).contains(&coord.col)
    }
}

/// Follow inserted and deleted rows and columns on the table's own sheet.
pub fn apply_line_edit(list: &mut [PivotTable], edit: &LineEdit) {
    for p in list {
        p.anchor = edit
            .map_coord(p.anchor)
            .unwrap_or_else(|| edit.axis.with(p.anchor, edit.at.min(edit.axis.max())));
        p.output = p.output.and_then(|o| edit.map_range(o));
    }
}

/// One source value: as shown, and its number if it is one.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub text: String,
    pub number: Option<f64>,
}

impl Item {
    pub fn text(s: &str) -> Self {
        Self {
            text: s.to_string(),
            number: None,
        }
    }

    pub fn number(n: f64) -> Self {
        Self {
            text: crate::format::format_general(n, 11),
            number: Some(n),
        }
    }
}

/// How blank items are labeled.
pub const BLANK: &str = "(blank)";

/// A cell of the computed table.
#[derive(Clone, Debug, PartialEq)]
pub enum PivotCell {
    Empty,
    Text(String),
    Number(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A report filter, or the gap after them
    Filter,
    Header,
    Data,
    Subtotal,
    GrandTotal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PivotOutput {
    /// Rows of cells, all the same width
    pub cells: Vec<Vec<PivotCell>>,
    pub kinds: Vec<RowKind>,
    /// For each column, the index of the value it shows, if any
    pub value_columns: Vec<Option<usize>>,
}

impl PivotOutput {
    pub fn width(&self) -> usize {
        self.value_columns.len()
    }
}

/// "Sum of Sales"
pub fn caption(value: &PivotValue, headers: &[String]) -> String {
    format!(
        "{} of {}",
        value.aggregate.label(),
        field_name(headers, value.field)
    )
}

pub fn field_name(headers: &[String], field: usize) -> String {
    headers
        .get(field)
        .filter(|h| !h.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| format!("Column {}", field + 1))
}

#[derive(Clone, Copy, Default)]
struct Acc {
    /// Non-blank items
    count: usize,
    numbers: usize,
    sum: f64,
    min: f64,
    max: f64,
}

impl Acc {
    fn add(&mut self, item: Option<&Item>) {
        let Some(item) = item else {
            return;
        };
        if !item.text.is_empty() {
            self.count += 1;
        }
        if let Some(n) = item.number {
            if self.numbers == 0 {
                (self.min, self.max) = (n, n);
            } else {
                self.min = self.min.min(n);
                self.max = self.max.max(n);
            }
            self.numbers += 1;
            self.sum += n;
        }
    }

    fn result(&self, aggregate: Aggregate) -> PivotCell {
        let has_numbers = self.numbers > 0;
        match aggregate {
            Aggregate::Sum => PivotCell::Number(self.sum),
            Aggregate::Count => PivotCell::Number(self.count as f64),
            Aggregate::Average if has_numbers => PivotCell::Number(self.sum / self.numbers as f64),
            Aggregate::Min if has_numbers => PivotCell::Number(self.min),
            Aggregate::Max if has_numbers => PivotCell::Number(self.max),
            _ => PivotCell::Empty,
        }
    }
}

/// The distinct items of a field in display order: numbers ascending, then
/// text A to Z, blanks last.
pub fn distinct_items<'a>(
    records: impl IntoIterator<Item = &'a Vec<Item>>,
    field: usize,
) -> Vec<String> {
    let mut seen: HashMap<&str, Option<f64>> = HashMap::new();
    for r in records {
        if let Some(item) = r.get(field) {
            seen.entry(item.text.as_str()).or_insert(item.number);
        }
    }
    let mut items: Vec<(&str, Option<f64>)> = seen.into_iter().collect();
    items.sort_by(|a, b| {
        let rank = |(t, n): &(&str, Option<f64>)| match (t.is_empty(), n) {
            (true, _) => 2,
            (false, Some(_)) => 0,
            (false, None) => 1,
        };
        rank(a).cmp(&rank(b)).then_with(|| match (a.1, b.1) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            _ => {
                a.0.to_lowercase()
                    .cmp(&b.0.to_lowercase())
                    .then(a.0.cmp(b.0))
            }
        })
    });
    items.into_iter().map(|(t, _)| t.to_string()).collect()
}

fn label(text: &str) -> PivotCell {
    PivotCell::Text(if text.is_empty() { BLANK } else { text }.to_string())
}

/// Lay out the table for `records` (the source rows under the header row).
pub fn compute(table: &PivotTable, headers: &[String], records: &[Vec<Item>]) -> PivotOutput {
    let text_of = |r: &Vec<Item>, f: usize| r.get(f).map_or("", |i| i.text.as_str()).to_string();
    let shown: Vec<&Vec<Item>> = records
        .iter()
        .filter(|r| {
            table
                .hidden
                .iter()
                .all(|(f, hidden)| !hidden.contains(&text_of(r, *f)))
        })
        .collect();

    // Each row and column item's position in display order.
    let mut order: HashMap<usize, HashMap<String, usize>> = HashMap::new();
    let mut names: HashMap<usize, Vec<String>> = HashMap::new();
    for &f in table.rows.iter().chain(&table.columns) {
        let items = distinct_items(shown.iter().copied(), f);
        order.insert(
            f,
            items
                .iter()
                .enumerate()
                .map(|(i, t)| (t.clone(), i))
                .collect(),
        );
        names.insert(f, items);
    }
    let key = |r: &Vec<Item>, fields: &[usize]| -> Vec<usize> {
        fields.iter().map(|f| order[f][&text_of(r, *f)]).collect()
    };
    let name = |f: usize, i: usize| names[&f][i].clone();

    // Totals for every (row key prefix, column key or all, value).
    let (kr, kc, nv) = (table.rows.len(), table.columns.len(), table.values.len());
    type Group = (Vec<usize>, Option<Vec<usize>>);
    let mut acc: HashMap<(Group, usize), Acc> = HashMap::new();
    let mut row_keys = BTreeSet::new();
    let mut col_keys = BTreeSet::new();
    for r in &shown {
        let rk = key(r, &table.rows);
        let ck = key(r, &table.columns);
        // The row itself, its outer group (subtotals) and everything; with
        // no row fields these are all the same group.
        let mut prefixes = vec![rk.clone()];
        if kr >= 1 {
            prefixes.push(Vec::new());
        }
        if kr >= 2 {
            prefixes.push(rk[..1].to_vec());
        }
        for prefix in prefixes {
            for col in [Some(ck.clone()), None] {
                for (v, value) in table.values.iter().enumerate() {
                    acc.entry(((prefix.clone(), col.clone()), v))
                        .or_default()
                        .add(r.get(value.field));
                }
            }
        }
        row_keys.insert(rk);
        col_keys.insert(ck);
    }
    let result = |prefix: &[usize], col: &Option<Vec<usize>>, v: usize| -> PivotCell {
        acc.get(&((prefix.to_vec(), col.clone()), v))
            .map_or(PivotCell::Empty, |a| a.result(table.values[v].aggregate))
    };

    // Columns: labels, then each column item x value, then grand totals.
    let label_cols = kr.max(1);
    let mut data_cols: Vec<(Option<Vec<usize>>, usize)> = Vec::new();
    let col_keys: Vec<Vec<usize>> = if kc == 0 {
        vec![Vec::new()]
    } else {
        col_keys.into_iter().collect()
    };
    for ck in &col_keys {
        for v in 0..nv {
            data_cols.push((Some(ck.clone()), v));
        }
    }
    if kc > 0 {
        for v in 0..nv {
            data_cols.push((None, v));
        }
    }
    let width = label_cols + data_cols.len();
    let mut value_columns = vec![None; label_cols];
    value_columns.extend(data_cols.iter().map(|(_, v)| Some(*v)));

    let mut cells: Vec<Vec<PivotCell>> = Vec::new();
    let mut kinds = Vec::new();
    let blank_row = || vec![PivotCell::Empty; width.max(2)];
    let caption_of = |v: usize| caption(&table.values[v], headers);

    // Report filters: the field and what it shows.
    for &f in &table.filters {
        let mut row = blank_row();
        row[0] = PivotCell::Text(field_name(headers, f));
        let items = distinct_items(records, f);
        let hidden = table.hidden.get(&f);
        let visible: Vec<&String> = items
            .iter()
            .filter(|i| hidden.is_none_or(|h| !h.contains(*i)))
            .collect();
        row[1] = match (hidden.is_none_or(|h| h.is_empty()), visible.as_slice()) {
            (true, _) => PivotCell::Text("(All)".into()),
            (false, [one]) => label(one),
            _ => PivotCell::Text("(Multiple Items)".into()),
        };
        cells.push(row);
        kinds.push(RowKind::Filter);
    }
    if !table.filters.is_empty() {
        cells.push(blank_row());
        kinds.push(RowKind::Filter);
    }

    // Headers. Without column fields: one row of field names and value
    // captions. With them: the caption and column field names, a row per
    // column field, and a row of captions under several values. The last
    // header row names the row fields.
    if kc == 0 {
        let mut row = vec![PivotCell::Empty; width];
        for (i, (_, v)) in data_cols.iter().enumerate() {
            row[label_cols + i] = PivotCell::Text(caption_of(*v));
        }
        cells.push(row);
        kinds.push(RowKind::Header);
    } else {
        let mut top = vec![PivotCell::Empty; width];
        if nv == 1 {
            top[0] = PivotCell::Text(caption_of(0));
        }
        if width > label_cols {
            let names: Vec<String> = table
                .columns
                .iter()
                .map(|&f| field_name(headers, f))
                .collect();
            top[label_cols] = PivotCell::Text(names.join(" / "));
        }
        cells.push(top);
        kinds.push(RowKind::Header);
        for level in 0..kc {
            let mut row = vec![PivotCell::Empty; width];
            for (i, (ck, v)) in data_cols.iter().enumerate() {
                let prev = i.checked_sub(1).map(|p| &data_cols[p].0);
                row[label_cols + i] = match ck {
                    // A column item, where its group starts.
                    Some(ck) => {
                        let starts = prev.is_none_or(|pk| match pk {
                            Some(pk) => pk[..=level] != ck[..=level],
                            None => true,
                        });
                        if starts {
                            label(&name(table.columns[level], ck[level]))
                        } else {
                            PivotCell::Empty
                        }
                    }
                    None if level == 0 => PivotCell::Text(if nv == 1 {
                        "Grand Total".into()
                    } else {
                        format!("Total {}", caption_of(*v))
                    }),
                    None => PivotCell::Empty,
                };
            }
            cells.push(row);
            kinds.push(RowKind::Header);
        }
        if nv > 1 {
            let mut row = vec![PivotCell::Empty; width];
            for (i, (ck, v)) in data_cols.iter().enumerate() {
                if ck.is_some() {
                    row[label_cols + i] = PivotCell::Text(caption_of(*v));
                }
            }
            cells.push(row);
            kinds.push(RowKind::Header);
        }
    }
    let last_header = cells.len() - 1;
    for (i, &f) in table.rows.iter().enumerate() {
        cells[last_header][i] = PivotCell::Text(field_name(headers, f));
    }

    // Data rows, with subtotals after each outer group.
    let values_row = |prefix: &[usize]| -> Vec<PivotCell> {
        data_cols
            .iter()
            .map(|(ck, v)| result(prefix, ck, *v))
            .collect()
    };
    let row_keys: Vec<Vec<usize>> = row_keys.into_iter().collect();
    let row_keys = if kr == 0 { vec![Vec::new()] } else { row_keys };
    for (i, rk) in row_keys.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| &row_keys[p]);
        let mut row = vec![PivotCell::Empty; label_cols];
        if kr == 0 {
            row[0] = PivotCell::Text("Total".into());
        }
        for level in 0..kr {
            if prev.is_none_or(|p| p[..=level] != rk[..=level]) {
                row[level] = label(&name(table.rows[level], rk[level]));
            }
        }
        row.extend(values_row(rk));
        cells.push(row);
        kinds.push(RowKind::Data);
        let group_ends = row_keys.get(i + 1).is_none_or(|next| next[0] != rk[0]);
        if kr >= 2 && group_ends {
            let mut row = vec![PivotCell::Empty; label_cols];
            let outer = name(table.rows[0], rk[0]);
            row[0] = PivotCell::Text(format!(
                "{} Total",
                if outer.is_empty() { BLANK } else { &outer }
            ));
            row.extend(values_row(&rk[..1]));
            cells.push(row);
            kinds.push(RowKind::Subtotal);
        }
    }
    if kr > 0 {
        let mut row = vec![PivotCell::Empty; label_cols];
        row[0] = PivotCell::Text("Grand Total".into());
        row.extend(values_row(&[]));
        cells.push(row);
        kinds.push(RowKind::GrandTotal);
    }

    // Filter rows may be wider than a narrow table.
    let width = cells.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut cells {
        row.resize(width, PivotCell::Empty);
    }
    value_columns.resize(width, None);
    PivotOutput {
        cells,
        kinds,
        value_columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers() -> Vec<String> {
        ["Region", "Product", "Units", "Price"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn records() -> Vec<Vec<Item>> {
        [
            ("North", "Pen", 10.0, 2.0),
            ("South", "Pen", 5.0, 2.0),
            ("North", "Lamp", 2.0, 30.0),
            ("North", "Pen", 4.0, 2.5),
            ("South", "Lamp", 1.0, 35.0),
            ("West", "", 3.0, 1.0),
        ]
        .iter()
        .map(|(r, p, u, pr)| {
            vec![
                Item::text(r),
                Item::text(p),
                Item::number(*u),
                Item::number(*pr),
            ]
        })
        .collect()
    }

    fn table() -> PivotTable {
        PivotTable::new(
            "PivotTable1".into(),
            "Data".into(),
            CellRange::from_a1("A1:D7").unwrap(),
            CellCoord::new(0, 0),
        )
    }

    fn t(s: &str) -> PivotCell {
        PivotCell::Text(s.into())
    }

    fn n(x: f64) -> PivotCell {
        PivotCell::Number(x)
    }

    const E: PivotCell = PivotCell::Empty;

    #[test]
    fn rows_and_values() {
        let mut p = table();
        p.rows = vec![0];
        p.values = vec![
            PivotValue {
                field: 2,
                aggregate: Aggregate::Sum,
            },
            PivotValue {
                field: 3,
                aggregate: Aggregate::Max,
            },
        ];
        let out = compute(&p, &headers(), &records());
        assert_eq!(
            out.cells,
            vec![
                vec![t("Region"), t("Sum of Units"), t("Max of Price")],
                vec![t("North"), n(16.0), n(30.0)],
                vec![t("South"), n(6.0), n(35.0)],
                vec![t("West"), n(3.0), n(1.0)],
                vec![t("Grand Total"), n(25.0), n(35.0)],
            ]
        );
        assert_eq!(
            out.kinds,
            vec![
                RowKind::Header,
                RowKind::Data,
                RowKind::Data,
                RowKind::Data,
                RowKind::GrandTotal
            ]
        );
        assert_eq!(out.value_columns, vec![None, Some(0), Some(1)]);
    }

    #[test]
    fn columns_subtotals_filters_and_hidden_items() {
        let mut p = table();
        p.rows = vec![0, 1];
        p.columns = vec![1];
        p.values = vec![PivotValue {
            field: 2,
            aggregate: Aggregate::Sum,
        }];
        p.filters = vec![0];
        p.hidden.insert(0, BTreeSet::from(["West".to_string()]));
        let out = compute(&p, &headers(), &records());
        assert_eq!(
            out.cells,
            vec![
                vec![t("Region"), t("(Multiple Items)"), E, E, E],
                vec![E, E, E, E, E],
                vec![t("Sum of Units"), E, t("Product"), E, E],
                vec![
                    t("Region"),
                    t("Product"),
                    t("Lamp"),
                    t("Pen"),
                    t("Grand Total")
                ],
                vec![t("North"), t("Lamp"), n(2.0), E, n(2.0)],
                vec![E, t("Pen"), E, n(14.0), n(14.0)],
                vec![t("North Total"), E, n(2.0), n(14.0), n(16.0)],
                vec![t("South"), t("Lamp"), n(1.0), E, n(1.0)],
                vec![E, t("Pen"), E, n(5.0), n(5.0)],
                vec![t("South Total"), E, n(1.0), n(5.0), n(6.0)],
                vec![t("Grand Total"), E, n(3.0), n(19.0), n(22.0)],
            ]
        );
    }

    #[test]
    fn counts_averages_and_blanks() {
        let mut p = table();
        p.rows = vec![1];
        p.values = vec![
            PivotValue {
                field: 0,
                aggregate: Aggregate::Count,
            },
            PivotValue {
                field: 3,
                aggregate: Aggregate::Average,
            },
        ];
        let out = compute(&p, &headers(), &records());
        assert_eq!(out.cells[1], vec![t("Lamp"), n(2.0), n(32.5)]);
        assert_eq!(
            out.cells[3],
            vec![t(BLANK), n(1.0), n(1.0)],
            "blank items last"
        );

        // Values only: one row of totals.
        p.rows.clear();
        let out = compute(&p, &headers(), &records());
        assert_eq!(
            out.cells,
            vec![
                vec![E, t("Count of Region"), t("Average of Price")],
                vec![t("Total"), n(6.0), n(72.5 / 6.0)],
            ]
        );
    }

    #[test]
    fn items_sort_numbers_then_text() {
        let recs: Vec<Vec<Item>> = [
            Item::text("b"),
            Item::number(10.0),
            Item::text("A"),
            Item::number(9.0),
            Item::text(""),
        ]
        .into_iter()
        .map(|i| vec![i])
        .collect();
        assert_eq!(distinct_items(&recs, 0), vec!["9", "10", "A", "b", ""]);
    }
}
