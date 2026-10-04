mod coord;
mod interner;
pub mod lines;
mod value;

pub use coord::{CellCoord, CellRange};
pub use interner::StringPool;
pub use lines::{Axis, LineEdit, MAX_COL, MAX_ROW};
pub use value::{CellError, CellValue};
