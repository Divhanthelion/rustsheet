//! Pivot tables saved by Excel, as RustSheet definitions so they can be
//! refreshed. A sheet's `xl/pivotTables/pivotTable1.xml` has the layout
//! (which fields are rows, columns, values and filters, which items are
//! hidden, where it sits); the cache definition it links to has the source
//! range and each field's items.

use super::limits;
use super::styles::{attr, part_rels};
use crate::cell::{CellCoord, CellRange};
use crate::pivot::{Aggregate, PivotTable, PivotValue};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::{BTreeMap, BTreeSet};

/// The pivot tables on each sheet, by sheet name. `sheets` lists each
/// sheet's name and part path.
pub(super) fn read_excel_pivots(
    read_bytes: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
    sheets: &[(String, String)],
) -> Vec<(String, Vec<PivotTable>)> {
    let text = |read_bytes: &mut dyn FnMut(&str) -> Option<Vec<u8>>, part: &str| {
        String::from_utf8(read_bytes(part)?).ok()
    };
    let mut out = Vec::new();
    for (name, path) in sheets {
        let mut list = Vec::new();
        for (_, kind, part) in part_rels(read_bytes, path) {
            if !kind.ends_with("/pivotTable") {
                continue;
            }
            let Some(layout) = text(read_bytes, &part) else {
                continue;
            };
            let Some(cache_part) = part_rels(read_bytes, &part)
                .into_iter()
                .find(|(_, k, _)| k.ends_with("/pivotCacheDefinition"))
                .map(|(_, _, target)| target)
            else {
                continue;
            };
            let Some(cache) = text(read_bytes, &cache_part) else {
                continue;
            };
            if let Some(table) = build(&layout, &cache) {
                list.push(table);
            }
        }
        if !list.is_empty() {
            out.push((name.clone(), list));
        }
    }
    out
}

/// A cache definition: the source and each field's items as shown.
#[derive(Debug, Default)]
struct Cache {
    sheet: Option<String>,
    source: Option<CellRange>,
    items: Vec<Vec<String>>,
}

fn item_text(kind: &[u8], v: Option<String>) -> String {
    let v = v.unwrap_or_default();
    match kind {
        b"n" => v
            .parse::<f64>()
            .map_or(v.clone(), |n| crate::format::format_general(n, 11)),
        b"b" => if v == "1" || v.eq_ignore_ascii_case("true") {
            "TRUE"
        } else {
            "FALSE"
        }
        .into(),
        b"d" => crate::format::parse_typed_number(v.get(..10).unwrap_or(&v))
            .map_or(v.clone(), |(serial, _)| {
                crate::format::format_number(serial, "m/d/yyyy").text
            }),
        b"m" => String::new(),
        _ => v,
    }
}

fn parse_cache(xml: &str) -> Cache {
    let mut cache = Cache::default();
    let mut reader = Reader::from_str(xml);
    let mut in_shared = false;
    loop {
        match reader.read_event() {
            // Only a non-empty <sharedItems> holds items.
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"sharedItems" => in_shared = true,
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"worksheetSource" => {
                    cache.sheet = attr(&e, b"sheet");
                    cache.source = attr(&e, b"ref").and_then(|r| limits::range(&r));
                }
                b"cacheField" => cache.items.push(Vec::new()),
                kind @ (b"s" | b"n" | b"b" | b"e" | b"d" | b"m") if in_shared => {
                    let text = item_text(kind, attr(&e, b"v"));
                    if let Some(items) = cache.items.last_mut() {
                        items.push(text);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if e.local_name().as_ref() == b"sharedItems" => in_shared = false,
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    cache
}

/// The table from its layout part and cache definition. `None` for sources
/// that aren't a range on a sheet (tables, names, external data).
fn build(layout: &str, cache: &str) -> Option<PivotTable> {
    let cache = parse_cache(cache);
    let source_sheet = cache.sheet.clone()?;
    let source = cache.source?;
    let fields = (source.end.col - source.start.col + 1) as usize;
    let field = |x: i64| usize::try_from(x).ok().filter(|&f| f < fields);

    let mut name = String::from("PivotTable");
    let mut location: Option<CellRange> = None;
    let mut page_rows: Option<u32> = None;
    let mut section = Vec::<u8>::new();
    // Hidden item indexes by pivot field
    let mut hidden_items: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut pivot_fields = 0usize;
    let (mut rows, mut columns, mut filters, mut values) = (vec![], vec![], vec![], vec![]);
    let mut page_items: Vec<(usize, usize)> = Vec::new();

    let mut reader = Reader::from_str(layout);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                match e.local_name().as_ref() {
                    b"pivotTableDefinition" => {
                        name = attr(&e, b"name").unwrap_or(name);
                    }
                    b"location" => {
                        location = attr(&e, b"ref").and_then(|r| limits::range(&r));
                        page_rows = attr(&e, b"rowPageCount").and_then(|v| v.parse().ok());
                    }
                    s @ (b"pivotFields" | b"rowFields" | b"colFields" | b"pageFields"
                    | b"dataFields") => {
                        section = s.to_vec();
                    }
                    // Counted at the start: an empty <pivotField/> has no end.
                    b"pivotField" => pivot_fields += 1,
                    b"item" if section == b"pivotFields" => {
                        let hidden = matches!(attr(&e, b"h").as_deref(), Some("1" | "true"));
                        let x = attr(&e, b"x").and_then(|x| x.parse().ok());
                        if let (true, Some(x), Some(f)) = (hidden, x, pivot_fields.checked_sub(1)) {
                            hidden_items.entry(f).or_default().push(x);
                        }
                    }
                    b"field" => {
                        let x = attr(&e, b"x").and_then(|x| x.parse::<i64>().ok());
                        // -2 is the "Values" pseudo-field.
                        if let Some(f) = x.and_then(field) {
                            match section.as_slice() {
                                b"rowFields" => rows.push(f),
                                b"colFields" => columns.push(f),
                                _ => {}
                            }
                        }
                    }
                    b"pageField" => {
                        if let Some(f) = attr(&e, b"fld")
                            .and_then(|x| x.parse::<i64>().ok())
                            .and_then(field)
                        {
                            filters.push(f);
                            if let Some(item) = attr(&e, b"item").and_then(|x| x.parse().ok()) {
                                page_items.push((f, item));
                            }
                        }
                    }
                    b"dataField" => {
                        if let Some(f) = attr(&e, b"fld")
                            .and_then(|x| x.parse::<i64>().ok())
                            .and_then(field)
                        {
                            let aggregate = match attr(&e, b"subtotal").as_deref() {
                                Some("count" | "countNums") => Aggregate::Count,
                                Some("average") => Aggregate::Average,
                                Some("min") => Aggregate::Min,
                                Some("max") => Aggregate::Max,
                                _ => Aggregate::Sum,
                            };
                            values.push(PivotValue {
                                field: f,
                                aggregate,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"pivotFields" | b"rowFields" | b"colFields" | b"pageFields" | b"dataFields" => {
                    section.clear()
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let location = location?;

    // Hidden items, by the text the cache gives them.
    let item = |f: usize, x: usize| cache.items.get(f).and_then(|items| items.get(x)).cloned();
    let mut hidden: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for (f, xs) in hidden_items {
        if f < fields {
            let set: BTreeSet<String> = xs.into_iter().filter_map(|x| item(f, x)).collect();
            if !set.is_empty() {
                hidden.insert(f, set);
            }
        }
    }
    // A filter showing one item hides the rest.
    for (f, x) in page_items {
        if let (Some(chosen), Some(all)) = (item(f, x), cache.items.get(f)) {
            hidden.insert(f, all.iter().filter(|i| **i != chosen).cloned().collect());
        }
    }
    // Filters sit above the table with a blank row between.
    let above = if filters.is_empty() {
        0
    } else {
        page_rows.unwrap_or(filters.len() as u32).saturating_add(1)
    };
    let anchor = CellCoord::new(location.start.row.saturating_sub(above), location.start.col);
    Some(PivotTable {
        name,
        source_sheet,
        source,
        anchor,
        rows,
        columns,
        values,
        filters,
        hidden,
        output: Some(CellRange::new(anchor, location.end)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CACHE: &str = r#"<pivotCacheDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" r:id="rId1" refreshOnLoad="1" recordCount="6">
  <cacheSource type="worksheet"><worksheetSource ref="A1:D7" sheet="Sales Data"/></cacheSource>
  <cacheFields count="4">
    <cacheField name="Region" numFmtId="0"><sharedItems count="3"><s v="North"/><s v="South"/><s v="West"/></sharedItems></cacheField>
    <cacheField name="Product" numFmtId="0"><sharedItems containsBlank="1" count="3"><s v="Pen"/><s v="Lamp"/><m/></sharedItems></cacheField>
    <cacheField name="Units" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" containsInteger="1" minValue="1" maxValue="10"/></cacheField>
    <cacheField name="Day" numFmtId="14"><sharedItems containsSemiMixedTypes="0" containsNonDate="0" containsDate="1" count="1"><d v="2026-01-05T00:00:00"/></sharedItems></cacheField>
  </cacheFields>
</pivotCacheDefinition>"#;

    const LAYOUT: &str = r#"<pivotTableDefinition xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" name="Sales by region" cacheId="3" dataCaption="Values">
  <location ref="A3:C7" firstHeaderRow="0" firstDataRow="1" firstDataCol="1" rowPageCount="1" colPageCount="1"/>
  <pivotFields count="4">
    <pivotField axis="axisRow" showAll="0"><items count="4"><item x="0"/><item x="1"/><item h="1" x="2"/><item t="default"/></items></pivotField>
    <pivotField axis="axisPage" showAll="0"><items count="4"><item x="0"/><item x="1"/><item x="2"/><item t="default"/></items></pivotField>
    <pivotField dataField="1" showAll="0"/>
    <pivotField showAll="0"/>
  </pivotFields>
  <rowFields count="1"><field x="0"/></rowFields>
  <colFields count="1"><field x="-2"/></colFields>
  <pageFields count="1"><pageField fld="1" item="1" hier="-1"/></pageFields>
  <dataFields count="2"><dataField name="Sum of Units" fld="2" baseField="0" baseItem="0"/><dataField name="Average of Units" fld="2" subtotal="average" baseField="0" baseItem="0"/></dataFields>
</pivotTableDefinition>"#;

    #[test]
    fn reads_excel_pivot_tables() {
        let cache = parse_cache(CACHE);
        assert_eq!(cache.items[1], vec!["Pen", "Lamp", ""]);
        assert_eq!(cache.items[3], vec!["1/5/2026"]);

        let p = build(LAYOUT, CACHE).unwrap();
        assert_eq!(p.name, "Sales by region");
        assert_eq!(p.source_sheet, "Sales Data");
        assert_eq!(p.source, CellRange::from_a1("A1:D7").unwrap());
        assert_eq!(p.rows, vec![0]);
        assert!(p.columns.is_empty(), "the Values pseudo-field is skipped");
        assert_eq!(p.filters, vec![1]);
        assert_eq!(
            p.values,
            vec![
                PivotValue {
                    field: 2,
                    aggregate: Aggregate::Sum
                },
                PivotValue {
                    field: 2,
                    aggregate: Aggregate::Average
                },
            ]
        );
        assert_eq!(p.hidden[&0], BTreeSet::from(["West".to_string()]));
        assert_eq!(
            p.hidden[&1],
            BTreeSet::from(["Pen".to_string(), String::new()]),
            "the filter shows only Lamp"
        );
        // One filter row and a gap above the table at A3.
        assert_eq!(p.anchor, CellCoord::new(0, 0));
        assert_eq!(p.output, Some(CellRange::from_a1("A1:C7").unwrap()));
    }

    #[test]
    fn other_sources_are_skipped() {
        let named = CACHE.replace(r#"ref="A1:D7" sheet="Sales Data""#, r#"name="SalesTable""#);
        assert!(build(LAYOUT, &named).is_none());
    }
}
