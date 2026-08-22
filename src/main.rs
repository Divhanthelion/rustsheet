use rustsheet::prelude::*;

fn main() {
    // Run GUI if the feature is enabled
    #[cfg(feature = "gui")]
    {
        // Check for --cli flag to run CLI demo instead
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--cli") {
            run_cli_demo();
            return;
        }

        println!("Starting RustSheet GUI...");
        if let Err(e) = rustsheet::gui::app::run() {
            eprintln!("Error running GUI: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // If GUI is not enabled, run CLI demo
    #[cfg(not(feature = "gui"))]
    run_cli_demo();
}

fn run_cli_demo() {
    println!("RustSheet - High-Performance Spreadsheet Engine");
    println!("================================================\n");

    // Demo: Create a simple spreadsheet with formulas
    demo_basic_operations();
    demo_formulas();

    #[cfg(feature = "xlsx")]
    demo_xlsx();
}

fn demo_basic_operations() {
    println!("=== Basic Operations ===\n");

    let mut sheet = Sheet::new("Demo");

    // Set some values
    let a1 = CellCoord::from_a1("A1").unwrap();
    let b1 = CellCoord::from_a1("B1").unwrap();
    let c1 = CellCoord::from_a1("C1").unwrap();

    sheet.set_number(a1, 100.0);
    sheet.set_number(b1, 200.0);
    sheet.set_text(c1, "Hello, RustSheet!");

    println!("Sheet: {}", sheet.name());
    println!("  A1 = {:?}", sheet.get(a1));
    println!("  B1 = {:?}", sheet.get(b1));

    if let Some(CellValue::Text(key)) = sheet.get(c1) {
        if let Some(text) = sheet.string_pool().resolve(*key) {
            println!("  C1 = \"{}\"", text);
        }
    }

    println!("  Cell count: {}", sheet.cell_count());
    println!();
}

fn demo_formulas() {
    println!("=== Formula Evaluation ===\n");

    let mut engine = CalcEngine::new();

    // Set up data
    println!("Setting up data:");
    println!("  A1 = 10");
    println!("  A2 = 20");
    println!("  A3 = 30");
    println!("  B1 = 5");

    engine.set_value(
        0,
        CellCoord::from_a1("A1").unwrap(),
        CellValueInput::Number(10.0),
    );
    engine.set_value(
        0,
        CellCoord::from_a1("A2").unwrap(),
        CellValueInput::Number(20.0),
    );
    engine.set_value(
        0,
        CellCoord::from_a1("A3").unwrap(),
        CellValueInput::Number(30.0),
    );
    engine.set_value(
        0,
        CellCoord::from_a1("B1").unwrap(),
        CellValueInput::Number(5.0),
    );

    // Test various formulas
    let formulas = [
        ("C1", "=A1+B1", "Addition"),
        ("C2", "=A1*A2", "Multiplication"),
        ("C3", "=SUM(A1:A3)", "SUM function"),
        ("C4", "=AVERAGE(A1:A3)", "AVERAGE function"),
        ("C5", "=MAX(A1:A3)", "MAX function"),
        ("C6", "=IF(A1>5,\"big\",\"small\")", "IF function"),
        ("C7", "=A1^2+B1^2", "Power operations"),
        ("C8", "=SQRT(C7)", "SQRT (depends on C7)"),
    ];

    println!("\nFormula results:");
    for (cell, formula, desc) in formulas {
        let coord = CellCoord::from_a1(cell).unwrap();
        if let Err(e) = engine.set_formula(0, coord, formula) {
            println!("  {} Error parsing {}: {}", cell, formula, e);
            continue;
        }
        let result = engine.get_value(0, coord);
        println!("  {} = {} => {:?}  ({})", cell, formula, result, desc);
    }

    // Demo dependency updates
    println!("\nDependency tracking:");
    println!("  Changing A1 from 10 to 100...");
    engine.set_value(
        0,
        CellCoord::from_a1("A1").unwrap(),
        CellValueInput::Number(100.0),
    );

    let c1_result = engine.get_value(0, CellCoord::from_a1("C1").unwrap());
    let c3_result = engine.get_value(0, CellCoord::from_a1("C3").unwrap());
    println!("  C1 (=A1+B1) now = {:?}", c1_result);
    println!("  C3 (=SUM(A1:A3)) now = {:?}", c3_result);

    // Demo error handling
    println!("\nError handling:");
    engine
        .set_formula(0, CellCoord::from_a1("D1").unwrap(), "=1/0")
        .unwrap();
    engine
        .set_formula(0, CellCoord::from_a1("D2").unwrap(), "=UNKNOWN()")
        .unwrap();

    println!(
        "  D1 = =1/0 => {:?}",
        engine.get_value(0, CellCoord::from_a1("D1").unwrap())
    );
    println!(
        "  D2 = =UNKNOWN() => {:?}",
        engine.get_value(0, CellCoord::from_a1("D2").unwrap())
    );

    // Demo cycle detection
    println!("\nCycle detection:");
    engine
        .set_formula(0, CellCoord::from_a1("E1").unwrap(), "=E2")
        .unwrap();
    engine
        .set_formula(0, CellCoord::from_a1("E2").unwrap(), "=E1")
        .unwrap();
    println!("  E1 = =E2, E2 = =E1");
    println!(
        "  E1 => {:?}",
        engine.get_value(0, CellCoord::from_a1("E1").unwrap())
    );

    println!();
}

#[cfg(feature = "xlsx")]
fn demo_xlsx() {
    println!("=== Excel File I/O ===\n");

    // Create a sheet and save it
    let mut sheet = Sheet::new("TestSheet");

    sheet.set_number(CellCoord::from_a1("A1").unwrap(), 1.0);
    sheet.set_number(CellCoord::from_a1("A2").unwrap(), 2.0);
    sheet.set_number(CellCoord::from_a1("A3").unwrap(), 3.0);
    sheet.set_text(CellCoord::from_a1("B1").unwrap(), "Hello");
    sheet.set_text(CellCoord::from_a1("B2").unwrap(), "World");
    sheet.set_bool(CellCoord::from_a1("C1").unwrap(), true);

    let mut writer = XlsxWriter::new();
    if let Err(e) = writer.add_sheet(&sheet) {
        println!("  Error adding sheet: {}", e);
        return;
    }

    let output_path = "test_output.xlsx";
    match writer.save(output_path) {
        Ok(_) => println!("  Saved workbook to: {}", output_path),
        Err(e) => println!("  Error saving workbook: {}", e),
    }

    println!();
}
