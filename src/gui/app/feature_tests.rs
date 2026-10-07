//! App-level tests for navigation, sheet operations, fill, find and recovery.

use super::*;
use crate::cell::{Axis, MAX_COL, MAX_ROW};
use crate::format::CellFormat;

fn app() -> SpreadsheetApp {
    let mut app = SpreadsheetApp::new();
    app.new_workbook();
    app
}

fn at(a1: &str) -> CellCoord {
    CellCoord::from_a1(a1).unwrap()
}

fn put(app: &mut SpreadsheetApp, a1: &str, content: &str) {
    app.set_cell_content(at(a1), content);
}

fn val(app: &SpreadsheetApp, a1: &str) -> CellResult {
    app.engine.get_value(app.current_sheet, at(a1))
}

fn text(s: &str) -> CellResult {
    CellResult::Text(s.into())
}

fn select(app: &mut SpreadsheetApp, from: &str, to: &str) {
    app.selection.move_to(at(from));
    app.selection.extend_to(at(to));
}

const VIEW: Vec2 = Vec2::new(1200.0, 800.0);

#[test]
fn ctrl_arrows_jump_to_data_edges() {
    let mut app = app();
    for a1 in ["A1", "A2", "A3", "A7", "A8"] {
        put(&mut app, a1, "1");
    }
    app.selection.move_to(at("A1"));
    app.navigate(NavigationKey::CtrlDown, false, VIEW);
    assert_eq!(app.selection.active, at("A3"), "end of the first block");
    app.navigate(NavigationKey::CtrlDown, false, VIEW);
    assert_eq!(app.selection.active, at("A7"), "start of the next block");
    app.navigate(NavigationKey::CtrlDown, false, VIEW);
    assert_eq!(app.selection.active, at("A8"));
    app.navigate(NavigationKey::CtrlDown, false, VIEW);
    assert_eq!(
        app.selection.active,
        CellCoord::new(MAX_ROW, 0),
        "sheet edge"
    );
    app.navigate(NavigationKey::CtrlUp, true, VIEW);
    assert_eq!(app.selection.active, at("A8"));
    assert_eq!(
        app.selection.anchor,
        CellCoord::new(MAX_ROW, 0),
        "shift extends"
    );

    put(&mut app, "D5", "x");
    app.navigate(NavigationKey::CtrlEnd, false, VIEW);
    assert_eq!(app.selection.active, at("D8"), "last used row and column");
}

#[test]
fn navigation_reaches_the_whole_sheet_and_skips_hidden_rows() {
    let mut app = app();
    app.selection.move_to(CellCoord::new(5000, 300));
    app.navigate(NavigationKey::Down, false, VIEW);
    assert_eq!(app.selection.active, CellCoord::new(5001, 300));
    app.engine.formatting_mut(0).hidden_rows.insert(5002);
    app.navigate(NavigationKey::Down, false, VIEW);
    assert_eq!(app.selection.active, CellCoord::new(5003, 300));
    app.selection.move_to(CellCoord::new(MAX_ROW, MAX_COL));
    app.navigate(NavigationKey::Right, false, VIEW);
    assert_eq!(app.selection.active, CellCoord::new(MAX_ROW, MAX_COL));
}

#[test]
fn header_clicks_select_whole_lines() {
    let mut app = app();
    app.handle_header_select(HeaderSelectFixture::col(2, false));
    let r = app.selection.primary_range();
    assert_eq!(
        (r.start, r.end),
        (CellCoord::new(0, 2), CellCoord::new(MAX_ROW, 2))
    );
    assert_eq!(
        app.selection.active,
        CellCoord::new(0, 2),
        "active is the top cell"
    );
    app.handle_header_select(HeaderSelectFixture::col(4, true));
    let r = app.selection.primary_range();
    assert_eq!((r.start.col, r.end.col), (2, 4));

    app.navigate(NavigationKey::SelectRow, false, VIEW);
    let r = app.selection.primary_range();
    assert_eq!((r.start.col, r.end.col), (0, MAX_COL));
}

/// Builds the grid's header click events.
struct HeaderSelectFixture;
impl HeaderSelectFixture {
    fn col(index: u32, extend: bool) -> crate::gui::grid::HeaderSelect {
        crate::gui::grid::HeaderSelect {
            axis: Axis::Column,
            index,
            extend,
        }
    }
}

#[test]
fn formatting_a_whole_column_uses_a_column_format() {
    let mut app = app();
    put(&mut app, "B2", "5");
    app.engine.set_cell_format(
        0,
        at("B3"),
        CellFormat {
            italic: true,
            ..Default::default()
        },
    );
    app.select_lines(Axis::Column, 1, 1);
    app.handle_format_action(FormatAction::ToggleBold);
    let f = app.engine.formatting(0).unwrap();
    assert!(f.column_formats.get(&1).is_some_and(|c| c.bold));
    assert!(f.cells().count() <= 1, "no per-cell formats were created");
    // A cell with its own format in that column gets the change too.
    let b3 = f.get(at("B3")).unwrap();
    assert!(b3.bold && b3.italic);
    assert!(f.effective(at("B900000")).is_some_and(|c| c.bold));

    app.undo();
    let f = app.engine.formatting(0).unwrap();
    assert!(f.column_formats.is_empty());
    assert!(!f.get(at("B3")).unwrap().bold);
}

#[test]
fn bolding_a_cell_keeps_its_column_fill() {
    let mut app = app();
    app.engine.formatting_mut(0).set_line_format(
        Axis::Column,
        0,
        CellFormat {
            fill: Some(crate::format::Rgb(255, 255, 0)),
            ..Default::default()
        },
    );
    select(&mut app, "A4", "A4");
    app.handle_format_action(FormatAction::ToggleBold);
    let a4 = app.engine.cell_format(0, at("A4")).unwrap();
    assert!(a4.bold && a4.fill.is_some());
    assert!(
        app.active_format().bold,
        "the toolbar shows what the cell shows"
    );
}

#[test]
fn inserting_and_deleting_rows_undo_as_one_step() {
    let mut app = app();
    put(&mut app, "A1", "1");
    put(&mut app, "A2", "2");
    put(&mut app, "A3", "=A1+A2");
    select(&mut app, "A2", "A3");
    app.insert_lines(Axis::Row);
    assert_eq!(val(&app, "A4"), CellResult::Value(2.0));
    assert_eq!(
        app.engine.get_formula(0, at("A5")).as_deref(),
        Some("=(A1+A4)")
    );
    assert_eq!(val(&app, "A5"), CellResult::Value(3.0));
    app.undo();
    assert_eq!(
        app.engine.get_formula(0, at("A3")).as_deref(),
        Some("=A1+A2")
    );
    assert_eq!(val(&app, "A5"), CellResult::Empty);

    select(&mut app, "A1", "A1");
    app.delete_lines(Axis::Row);
    assert_eq!(val(&app, "A1"), CellResult::Value(2.0));
    assert_eq!(
        val(&app, "A2"),
        CellResult::Error(crate::cell::CellError::Ref)
    );
    app.undo();
    assert_eq!(val(&app, "A3"), CellResult::Value(3.0));
}

#[test]
fn hiding_and_unhiding_columns() {
    let mut app = app();
    select(&mut app, "B1", "C1");
    app.set_lines_hidden(Axis::Column, true);
    assert!(app.grid_config.is_hidden(Axis::Column, 1));
    assert!(app.grid_config.is_hidden(Axis::Column, 2));
    select(&mut app, "A1", "D1");
    app.set_lines_hidden(Axis::Column, false);
    assert!(!app.grid_config.is_hidden(Axis::Column, 1));
    app.undo();
    assert!(app.grid_config.is_hidden(Axis::Column, 2));
}

#[test]
fn quick_sort_finds_the_data_and_its_header() {
    let mut app = app();
    for (a1, v) in [
        ("A1", "Name"),
        ("B1", "Qty"),
        ("A2", "pear"),
        ("B2", "3"),
        ("A3", "apple"),
        ("B3", "9"),
    ] {
        put(&mut app, a1, v);
    }
    app.selection.move_to(at("B2"));
    app.quick_sort(false);
    assert_eq!(val(&app, "A1"), text("Name"), "header stays");
    assert_eq!(val(&app, "A2"), text("apple"), "9 sorts first descending");
    app.undo();
    assert_eq!(val(&app, "A2"), text("pear"));
}

#[test]
fn filter_on_and_off() {
    let mut app = app();
    for (a1, v) in [
        ("A1", "Fruit"),
        ("A2", "Tea"),
        ("A3", "Cake"),
        ("A4", "Tea"),
    ] {
        put(&mut app, a1, v);
    }
    app.selection.move_to(at("A2"));
    app.toggle_filter();
    let range = app
        .engine
        .formatting(0)
        .unwrap()
        .filter
        .as_ref()
        .unwrap()
        .range;
    assert_eq!(range, CellRange::from_a1("A1:A4").unwrap());

    app.open_filter_popup(0, egui::Pos2::ZERO);
    let mut popup = app.filter_popup.take().unwrap();
    for (v, on) in &mut popup.values {
        *on = v == "Tea";
    }
    app.filter_popup = Some(popup);
    let popup = app.filter_popup.take().unwrap();
    app.apply_filter_popup(&popup);
    assert!(app.grid_config.is_hidden(Axis::Row, 2), "Cake is hidden");
    assert!(!app.grid_config.is_hidden(Axis::Row, 1));

    app.toggle_filter();
    assert!(app.engine.formatting(0).unwrap().filter.is_none());
    assert!(!app.grid_config.is_hidden(Axis::Row, 2), "rows come back");
}

#[test]
fn fill_handle_continues_series() {
    let mut app = app();
    put(&mut app, "A1", "2");
    put(&mut app, "A2", "4");
    select(&mut app, "A1", "A2");
    app.fill_to(at("A5"));
    assert_eq!(val(&app, "A5"), CellResult::Value(10.0));

    put(&mut app, "B1", "Item 9");
    select(&mut app, "B1", "B1");
    app.fill_to(at("B3"));
    assert_eq!(val(&app, "B3"), text("Item 11"));

    put(&mut app, "C1", "Mar");
    select(&mut app, "C1", "C1");
    app.fill_to(at("F1"));
    assert_eq!(val(&app, "F1"), text("Jun"));

    put(&mut app, "D3", "2023-03-15");
    select(&mut app, "D3", "D3");
    app.fill_to(at("D5"));
    assert_eq!(val(&app, "D5"), CellResult::Value(45002.0));
    assert!(
        app.engine
            .cell_format(0, at("D5"))
            .is_some_and(|f| f.number_format.is_some())
    );

    // A single number repeats, as in Excel.
    put(&mut app, "E1", "7");
    select(&mut app, "E1", "E1");
    app.fill_to(at("E3"));
    assert_eq!(val(&app, "E3"), CellResult::Value(7.0));

    // Upward fill counts down.
    put(&mut app, "G5", "10");
    put(&mut app, "G6", "20");
    select(&mut app, "G5", "G6");
    app.fill_to(at("G3"));
    assert_eq!(val(&app, "G3"), CellResult::Value(-10.0));

    app.undo();
    assert_eq!(val(&app, "G3"), CellResult::Empty);
}

#[test]
fn fill_down_copies_formulas() {
    let mut app = app();
    put(&mut app, "A1", "1");
    put(&mut app, "A2", "2");
    put(&mut app, "B1", "=A1*10");
    select(&mut app, "B1", "B2");
    app.fill_down();
    assert_eq!(val(&app, "B2"), CellResult::Value(20.0));
    // A one-row selection copies from the row above.
    select(&mut app, "B3", "B3");
    put(&mut app, "A3", "3");
    app.fill_down();
    assert_eq!(val(&app, "B3"), CellResult::Value(30.0));
}

#[test]
fn find_and_replace_across_the_workbook() {
    let mut app = app();
    put(&mut app, "A1", "Green tea");
    put(&mut app, "C5", "tea time");
    app.add_sheet();
    put(&mut app, "B2", "TEA");
    app.switch_sheet(0);

    app.open_find(true);
    {
        let d = app.find_dialog.as_mut().unwrap();
        d.query = "tea".into();
        d.whole_workbook = true;
        d.replacement = "coffee".into();
    }
    let query = find::Query::new("tea", false, false);
    let hits = app.find_all(&query, true, true);
    assert_eq!(hits.len(), 3);
    let n = app.replace_everywhere();
    assert_eq!(n, 3);
    assert_eq!(val(&app, "A1"), text("Green coffee"));
    assert_eq!(app.engine.get_value(1, at("B2")), text("coffee"));
    app.undo();
    assert_eq!(app.engine.get_value(1, at("B2")), text("TEA"));
}

#[test]
fn merging_keeps_the_top_left_value_and_unmerges() {
    let mut app = app();
    put(&mut app, "A1", "Title");
    select(&mut app, "A1", "C1");
    app.toggle_merge();
    let merges = app.engine.formatting(0).unwrap().merges.clone();
    assert_eq!(merges, vec![CellRange::from_a1("A1:C1").unwrap()]);
    assert_eq!(app.snap_to_merge(at("B1")), at("A1"));
    // Moving right from the merge leaves it.
    app.selection.move_to(at("A1"));
    app.navigate(NavigationKey::Right, false, VIEW);
    assert_eq!(app.selection.active, at("D1"));
    select(&mut app, "B1", "B1");
    app.toggle_merge();
    assert!(app.engine.formatting(0).unwrap().merges.is_empty());
}

#[test]
fn freezing_panes() {
    let mut app = app();
    app.freeze(1, 2);
    assert_eq!(
        (app.grid_config.frozen_rows, app.grid_config.frozen_cols),
        (1, 2)
    );
    app.navigate(NavigationKey::CtrlHome, false, VIEW);
    assert_eq!(
        app.selection.active,
        CellCoord::new(1, 2),
        "first unfrozen cell"
    );
}

#[test]
fn whole_column_copy_and_delete_stay_within_the_data() {
    let mut app = app();
    put(&mut app, "A1", "1");
    put(&mut app, "A3", "3");
    app.select_lines(Axis::Column, 0, 0);
    let ctx = egui::Context::default();
    app.copy_selection(&ctx, false);
    let clip = app.clipboard.as_ref().unwrap();
    assert_eq!(clip.rows, 3, "trimmed to the used rows");
    app.delete_selection();
    assert_eq!(val(&app, "A3"), CellResult::Empty);
}

#[test]
fn autosave_and_recovery_round_trip() {
    let dir = std::env::temp_dir().join(format!("rustsheet_recovery_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Recovery")).unwrap();
    let xlsx = dir.join("Recovery").join("crashed.xlsx");
    let meta = dir.join("Recovery").join("crashed.json");

    // A session that autosaved, then died long ago.
    let mut old = app();
    put(&mut old, "A1", "unsaved work");
    old.write_xlsx(&xlsx).unwrap();
    let stale = recovery::RecoveryMeta {
        original: Some(PathBuf::from("C:/Budget.xlsx")),
        saved_at: 1,
        heartbeat: 1,
    };
    std::fs::write(&meta, serde_json::to_string(&stale).unwrap()).unwrap();

    let found = recovery::find_recoverable_in(&dir.join("Recovery"));
    assert_eq!(found.len(), 1);
    let mut app = app();
    app.load_file(&found[0].xlsx);
    assert_eq!(val(&app, "A1"), text("unsaved work"));

    // A live session (fresh heartbeat) is not offered.
    let live = recovery::RecoveryMeta {
        heartbeat: u64::MAX / 2,
        ..stale
    };
    std::fs::write(&meta, serde_json::to_string(&live).unwrap()).unwrap();
    assert!(recovery::find_recoverable_in(&dir.join("Recovery")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saves_are_atomic_and_leave_no_temp_files() {
    let dir = std::env::temp_dir().join(format!("rustsheet_atomic_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("book.xlsx");
    let mut app = app();
    put(&mut app, "A1", "v1");
    app.save_to_path(&path);
    put(&mut app, "A1", "v2");
    app.save_to_path(&path);
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["book.xlsx".to_string()]);
    let mut back = SpreadsheetApp::new();
    back.load_file(&path);
    assert_eq!(val(&back, "A1"), text("v2"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn note(text: &str) -> crate::format::Note {
    crate::format::Note {
        text: text.into(),
        author: None,
    }
}

#[test]
fn notes_undo_copy_sort_and_move_with_rows() {
    let mut app = app();
    put(&mut app, "A1", "b");
    put(&mut app, "A2", "a");
    app.selection.move_to(at("A1"));
    app.set_note(at("A1"), Some(note("check")));
    assert_eq!(
        app.note_at(at("A1")).map(|n| n.text.as_str()),
        Some("check")
    );
    app.undo();
    assert!(app.note_at(at("A1")).is_none());
    app.redo();

    // Copy and paste carries the note.
    let ctx = egui::Context::default();
    select(&mut app, "A1", "A1");
    app.copy_selection(&ctx, false);
    select(&mut app, "C5", "C5");
    let text = app.clipboard.as_ref().unwrap().text.clone();
    app.paste_text(&text);
    assert!(app.note_at(at("C5")).is_some());
    app.undo();
    assert!(app.note_at(at("C5")).is_none());

    // Sorting moves the note with its row ("b" sorts after "a").
    select(&mut app, "A1", "A2");
    app.quick_sort(true);
    assert_eq!(val(&app, "A2"), text_value("b"));
    assert!(app.note_at(at("A2")).is_some());
    assert!(app.note_at(at("A1")).is_none());

    // Inserting a row above moves it down.
    select(&mut app, "A1", "A1");
    app.insert_lines(Axis::Row);
    assert!(app.note_at(at("A3")).is_some());
}

fn text_value(s: &str) -> CellResult {
    CellResult::Text(s.into())
}

#[test]
fn typed_entries_are_checked_against_validation() {
    use crate::format::validation::{CompareOp, DataValidation, ErrorStyle, ValidationKind};
    let mut app = app();
    app.engine
        .formatting_mut(0)
        .validations
        .push(DataValidation {
            ranges: vec![CellRange::from_a1("A1:A9").unwrap()],
            kind: ValidationKind::Whole,
            operator: CompareOp::Between,
            formula1: "1".into(),
            formula2: Some("10".into()),
            ..Default::default()
        });
    let type_in = |app: &mut SpreadsheetApp, a1: &str, text: &str| {
        app.selection.move_to(at(a1));
        app.input_mode = InputMode::Editing { cell: at(a1) };
        app.edit_buffer = text.into();
        app.confirm_edit(true, false);
    };

    type_in(&mut app, "A1", "5");
    assert_eq!(val(&app, "A1"), CellResult::Value(5.0));

    // Stop + Retry: the entry is undone and editing resumes with the text.
    app.alert_answer = Some(validation_ui::AlertAnswer::Retry);
    type_in(&mut app, "A2", "50");
    assert_eq!(val(&app, "A2"), CellResult::Empty);
    assert_eq!(app.retry_text.as_deref(), Some("50"));
    assert!(matches!(app.input_mode, InputMode::TransitionToEdit { .. }));
    app.retry_text = None;
    app.input_mode = InputMode::Navigation;

    // Cancel undoes it; nothing lands in the undo history.
    app.alert_answer = Some(validation_ui::AlertAnswer::Cancel);
    type_in(&mut app, "A3", "abc");
    assert_eq!(val(&app, "A3"), CellResult::Empty);

    // A warning the user accepts keeps the value.
    app.engine.formatting_mut(0).validations[0].error_style = ErrorStyle::Warning;
    app.alert_answer = Some(validation_ui::AlertAnswer::Keep);
    type_in(&mut app, "A4", "99");
    assert_eq!(val(&app, "A4"), CellResult::Value(99.0));

    // Choosing from a list popup writes the item.
    app.engine
        .formatting_mut(0)
        .validations
        .push(DataValidation {
            ranges: vec![CellRange::from_a1("B1").unwrap()],
            kind: ValidationKind::List,
            formula1: "\"Yes,No\"".into(),
            ..Default::default()
        });
    app.selection.move_to(at("B1"));
    app.open_list_popup(egui::Pos2::ZERO);
    assert_eq!(app.list_popup.as_ref().unwrap().items, vec!["Yes", "No"]);
}

#[test]
fn conditional_formatting_from_the_dialog() {
    use crate::format::validation::CompareOp;
    let mut app = app();
    for (a1, v) in [("A1", "5"), ("A2", "50"), ("A3", "500")] {
        put(&mut app, a1, v);
    }
    select(&mut app, "A1", "A3");
    app.open_conditional_dialog();
    let d = app.cf_dialog.as_mut().unwrap();
    let editor = d.editor.as_mut().expect("starts with a new rule");
    assert_eq!(editor.applies_to, "A1:A3");
    editor.op = CompareOp::Greater;
    editor.value1 = "10".into();
    let rule = editor.build().unwrap();
    app.cf_dialog = None;
    app.apply_conditional(vec![rule]);

    let look = |app: &SpreadsheetApp, a1: &str| app.engine.conditional_look(0, at(a1));
    assert!(look(&app, "A1").is_none());
    assert!(look(&app, "A2").is_some());

    // Inserting a row above moves the rule with its cells.
    select(&mut app, "A1", "A1");
    app.insert_lines(Axis::Row);
    assert!(look(&app, "A3").is_some());
    app.undo();
    app.undo();
    assert!(
        app.engine
            .formatting(0)
            .is_none_or(|f| f.conditional.is_empty())
    );

    // With a rule on the selection, the dialog opens on the rules list.
    app.redo();
    select(&mut app, "A2", "A2");
    app.open_conditional_dialog();
    assert!(app.cf_dialog.as_ref().unwrap().editor.is_none());
}

#[test]
fn conditional_dialog_draws_every_rule_kind() {
    use super::conditional_ui::RuleKind;
    let mut app = app();
    put(&mut app, "A1", "1");
    select(&mut app, "A1", "A3");
    app.open_conditional_dialog();
    let ctx = egui::Context::default();
    for kind in [
        RuleKind::CellValue,
        RuleKind::Text,
        RuleKind::Top,
        RuleKind::Average,
        RuleKind::Duplicate,
        RuleKind::Blanks,
        RuleKind::Errors,
        RuleKind::Formula,
        RuleKind::TwoColorScale,
        RuleKind::ThreeColorScale,
        RuleKind::DataBar,
    ] {
        app.cf_dialog
            .as_mut()
            .unwrap()
            .editor
            .as_mut()
            .unwrap()
            .kind = kind;
        let _ = ctx.run(Default::default(), |ctx| app.show_conditional_dialog(ctx));
        assert!(app.cf_dialog.is_some());
        let rule = app
            .cf_dialog
            .as_ref()
            .unwrap()
            .editor
            .as_ref()
            .unwrap()
            .build();
        // Kinds with a value to type say what's missing; the rest are complete.
        let needs_input = matches!(
            kind,
            RuleKind::CellValue | RuleKind::Text | RuleKind::Formula
        );
        assert_eq!(rule.is_err(), needs_input, "{kind:?}");
        if let Ok(rule) = rule {
            app.cf_dialog.as_mut().unwrap().rules.push(rule);
        }
    }
    // The rules list, with a sample of each.
    app.cf_dialog.as_mut().unwrap().editor = None;
    let _ = ctx.run(Default::default(), |ctx| app.show_conditional_dialog(ctx));
    assert_eq!(app.cf_dialog.as_ref().unwrap().rules.len(), 8);
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::new(width, height)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[test]
fn pictures_insert_move_order_and_delete() {
    use crate::gui::grid::{PictureAction, PicturePlace};
    let mut app = app();
    app.last_viewport = Vec2::new(1000.0, 800.0);
    select(&mut app, "B3", "B3");
    app.insert_picture(png(40, 20)).unwrap();
    assert!(app.insert_picture(b"not a picture".to_vec()).is_err());
    assert_eq!(app.sheet_pictures().len(), 1);
    let p = &app.sheet_pictures()[0];
    assert_eq!((p.anchor, p.size), (at("B3"), (50.0, 25.0)));
    assert_eq!(app.selected_picture, Some(0));

    // Big pictures start scaled to fit the window.
    app.insert_picture(png(1600, 400)).unwrap();
    let big = app.sheet_pictures()[1].size;
    assert!(
        big.0 <= 600.0 && (big.0 / big.1 - 4.0).abs() < 0.01,
        "{big:?}"
    );

    app.place_picture(
        0,
        PicturePlace {
            anchor: at("D5"),
            offset: (3.0, 4.0),
            size: (100.0, 50.0),
        },
    );
    assert_eq!(app.sheet_pictures()[0].anchor, at("D5"));

    // It hangs from its cell when rows are inserted above.
    select(&mut app, "A1", "A1");
    app.insert_lines(Axis::Row);
    assert_eq!(app.sheet_pictures()[0].anchor, at("D6"));

    app.picture_action(0, PictureAction::BringToFront);
    assert_eq!(app.sheet_pictures()[1].anchor, at("D6"));
    assert_eq!(app.selected_picture, Some(1));
    app.picture_action(1, PictureAction::ResetSize);
    assert_eq!(app.sheet_pictures()[1].size, (50.0, 25.0));

    app.picture_action(1, PictureAction::Delete);
    assert_eq!(app.sheet_pictures().len(), 1);
    app.undo();
    assert_eq!(app.sheet_pictures().len(), 2);

    // Textures for the grid.
    let ctx = egui::Context::default();
    let pictures = app.sheet_pictures().to_vec();
    app.picture_textures.update(&ctx, &pictures);
    assert_eq!(app.picture_textures.ids.len(), 2);
    app.picture_textures.update(&ctx, &pictures[..1]);
    assert_eq!(app.picture_textures.ids.len(), 1);
}

#[test]
fn pivot_tables_create_refresh_and_undo() {
    use super::pivot_ui::Area;
    let mut app = app();
    for (a1, v) in [
        ("A1", "Region"),
        ("B1", "Sales"),
        ("A2", "North"),
        ("B2", "10"),
        ("A3", "South"),
        ("B3", "5"),
        ("A4", "North"),
        ("B4", "7"),
    ] {
        put(&mut app, a1, v);
    }
    select(&mut app, "A2", "A2");
    app.open_pivot_dialog(false);
    let mut d = app.pivot_dialog.take().unwrap();
    assert_eq!(d.source, "Sheet1!A1:B4", "the block around the active cell");
    d.load(&app.engine, "Sheet1");
    d.add(0, Area::Rows);
    d.add(1, Area::Values);
    app.apply_pivot_dialog(&mut d).unwrap();

    assert_eq!(app.sheet_names[1], "Pivot1");
    assert_eq!(app.current_sheet, 1);
    let column = |app: &SpreadsheetApp, col: char| -> Vec<CellResult> {
        (1..=4).map(|r| val(app, &format!("{col}{r}"))).collect()
    };
    assert_eq!(
        column(&app, 'A'),
        vec![
            text("Region"),
            text("North"),
            text("South"),
            text("Grand Total")
        ]
    );
    assert_eq!(
        column(&app, 'B'),
        vec![
            text("Sum of Sales"),
            CellResult::Value(17.0),
            CellResult::Value(5.0),
            CellResult::Value(22.0)
        ]
    );
    assert!(app.engine.cell_format(1, at("A1")).is_some_and(|f| f.bold));

    // The data changes; Refresh All catches up, and undo puts it back.
    app.engine
        .set_value(0, at("B3"), CellValueInput::Number(50.0));
    app.refresh_all_pivots();
    assert_eq!(val(&app, "B3"), CellResult::Value(50.0));
    app.undo();
    assert_eq!(val(&app, "B3"), CellResult::Value(5.0));

    // A field in columns grows the table; the old cells are replaced.
    select(&mut app, "A2", "A2");
    app.open_pivot_dialog(true);
    let mut d = app.pivot_dialog.take().unwrap();
    d.load(&app.engine, "Pivot1");
    d.add(0, Area::Columns);
    app.apply_pivot_dialog(&mut d).unwrap();
    assert_eq!(val(&app, "C2"), text("South"));

    // It won't write over other data.
    put(&mut app, "J1", "keep me");
    app.switch_sheet(0);
    select(&mut app, "D1", "D1");
    app.open_pivot_dialog(false);
    let mut d = app.pivot_dialog.take().unwrap();
    d.source = "Sheet1!A1:B4".into();
    d.load(&app.engine, "Sheet1");
    d.add(0, Area::Rows);
    d.new_sheet = false;
    d.destination = "A2".into();
    assert!(app.apply_pivot_dialog(&mut d).is_err());
    assert_eq!(app.engine.formatting(0).map_or(0, |f| f.pivots.len()), 0);

    // Deleting removes its cells.
    app.switch_sheet(1);
    app.delete_pivot(1, 0);
    assert_eq!(val(&app, "A1"), CellResult::Empty);
    assert_eq!(val(&app, "J1"), text("keep me"));
}

#[test]
fn sheet_names_follow_excel_rules() {
    let mut app = app();
    app.add_sheet();
    app.rename_sheet(1, "  Q1 sales ".into());
    assert_eq!(app.sheet_names[1], "Q1 sales");
    let long = "x".repeat(32);
    for bad in ["", "a/b", "what?", long.as_str(), "'quoted'", "SHEET1"] {
        app.rename_sheet(1, bad.into());
        assert_eq!(app.sheet_names[1], "Q1 sales", "{bad:?} is refused");
    }
}

#[test]
fn pasted_images_become_pictures() {
    let mut app = app();
    app.last_viewport = Vec2::new(1000.0, 800.0);
    select(&mut app, "C2", "C2");
    app.paste_image(arboard::ImageData {
        width: 4,
        height: 2,
        bytes: std::borrow::Cow::Owned(vec![200; 4 * 2 * 4]),
    });
    let p = &app.sheet_pictures()[0];
    assert_eq!(p.kind, crate::format::picture::PictureKind::Png);
    assert_eq!((p.anchor, p.size), (at("C2"), (5.0, 2.5)));
}

#[test]
fn time_zone_setting_moves_now_and_today() {
    use crate::calc::functions::date_to_serial;
    use crate::gui::settings::TimeZoneChoice;
    /// 2024-07-01 22:00 UTC.
    fn evening() -> f64 {
        let days = date_to_serial(2024, 7, 1) - date_to_serial(1970, 1, 1);
        days * 86_400.0 + 22.0 * 3_600.0
    }
    let (july_1, july_2) = (date_to_serial(2024, 7, 1), date_to_serial(2024, 7, 2));
    let now = |app: &SpreadsheetApp| match val(app, "B1") {
        CellResult::Value(n) => n,
        other => panic!("NOW() is {other:?}"),
    };
    let mut app = app();
    app.engine.set_clock(evening);
    put(&mut app, "A1", "=TODAY()");
    put(&mut app, "B1", "=NOW()");
    let ctx = egui::Context::default();

    app.run_command(&ctx, Command::TimeZone(TimeZoneChoice::Utc));
    assert_eq!(app.settings.time_zone, TimeZoneChoice::Utc);
    assert_eq!(val(&app, "A1"), CellResult::Value(july_1));
    assert!((now(&app) - (july_1 + 22.0 / 24.0)).abs() < 1e-9);

    // UTC+02:30 is half past midnight on the 2nd.
    app.run_command(&ctx, Command::TimeZone(TimeZoneChoice::Fixed(150)));
    assert_eq!(app.engine.clock_offset(), 150);
    assert_eq!(val(&app, "A1"), CellResult::Value(july_2));
    assert!((now(&app) - (july_2 + 0.5 / 24.0)).abs() < 1e-9);

    app.run_command(&ctx, Command::TimeZone(TimeZoneChoice::Windows));
    assert_eq!(
        app.engine.clock_offset(),
        TimeZoneChoice::Windows.offset_minutes()
    );

    // A workbook opened or started afresh gets a new engine, at UTC until
    // the next frame catches it up.
    app.settings.time_zone = TimeZoneChoice::Fixed(-300);
    app.new_workbook();
    assert_eq!(app.engine.clock_offset(), 0);
    app.sync_clock();
    assert_eq!(app.engine.clock_offset(), -300);
}
