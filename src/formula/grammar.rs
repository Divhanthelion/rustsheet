use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "formula/formula.pest"]
pub struct FormulaGrammar;
