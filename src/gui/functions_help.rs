//! Function help database and autocomplete support

/// Function metadata for help and autocomplete
#[derive(Clone)]
pub struct FunctionInfo {
    pub name: &'static str,
    pub category: FunctionCategory,
    pub syntax: &'static str,
    pub description: &'static str,
    pub examples: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FunctionCategory {
    Math,
    Statistical,
    Logical,
    Text,
    Lookup,
    DateTime,
    Info,
}

impl FunctionCategory {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Math => "Math & Trig",
            Self::Statistical => "Statistical",
            Self::Logical => "Logical",
            Self::Text => "Text",
            Self::Lookup => "Lookup & Reference",
            Self::DateTime => "Date & Time",
            Self::Info => "Information",
        }
    }

    pub fn all() -> &'static [FunctionCategory] {
        &[
            Self::Math,
            Self::Statistical,
            Self::Logical,
            Self::Text,
            Self::Lookup,
            Self::DateTime,
            Self::Info,
        ]
    }
}

/// Get all function definitions
pub fn get_all_functions() -> &'static [FunctionInfo] {
    &FUNCTIONS
}

/// Get functions matching a prefix (for autocomplete)
pub fn get_matching_functions(prefix: &str) -> Vec<&'static FunctionInfo> {
    let prefix_upper = prefix.to_uppercase();
    FUNCTIONS
        .iter()
        .filter(|f| f.name.starts_with(&prefix_upper))
        .collect()
}

/// Get function by exact name
pub fn get_function(name: &str) -> Option<&'static FunctionInfo> {
    let name_upper = name.to_uppercase();
    FUNCTIONS.iter().find(|f| f.name == name_upper)
}

// Complete function database
static FUNCTIONS: &[FunctionInfo] = &[
    // ===== Math Functions =====
    FunctionInfo {
        name: "SUM",
        category: FunctionCategory::Math,
        syntax: "SUM(number1, [number2], ...)",
        description: "Adds all numbers in a range of cells",
        examples: &["=SUM(A1:A10)", "=SUM(1, 2, 3)", "=SUM(A1, B1, C1)"],
    },
    FunctionInfo {
        name: "ABS",
        category: FunctionCategory::Math,
        syntax: "ABS(number)",
        description: "Returns the absolute value of a number",
        examples: &["=ABS(-5)", "=ABS(A1)"],
    },
    FunctionInfo {
        name: "ROUND",
        category: FunctionCategory::Math,
        syntax: "ROUND(number, [num_digits])",
        description: "Rounds a number to a specified number of digits",
        examples: &["=ROUND(2.567, 2)", "=ROUND(A1, 0)"],
    },
    FunctionInfo {
        name: "ROUNDUP",
        category: FunctionCategory::Math,
        syntax: "ROUNDUP(number, num_digits)",
        description: "Rounds a number up, away from zero",
        examples: &["=ROUNDUP(3.14159, 2)", "=ROUNDUP(-3.1, 0)"],
    },
    FunctionInfo {
        name: "ROUNDDOWN",
        category: FunctionCategory::Math,
        syntax: "ROUNDDOWN(number, num_digits)",
        description: "Rounds a number down, toward zero",
        examples: &["=ROUNDDOWN(3.999, 0)", "=ROUNDDOWN(-3.9, 0)"],
    },
    FunctionInfo {
        name: "SQRT",
        category: FunctionCategory::Math,
        syntax: "SQRT(number)",
        description: "Returns the square root of a number",
        examples: &["=SQRT(16)", "=SQRT(A1)"],
    },
    FunctionInfo {
        name: "POWER",
        category: FunctionCategory::Math,
        syntax: "POWER(number, power)",
        description: "Returns the result of a number raised to a power",
        examples: &["=POWER(2, 8)", "=POWER(A1, 2)"],
    },
    FunctionInfo {
        name: "MOD",
        category: FunctionCategory::Math,
        syntax: "MOD(number, divisor)",
        description: "Returns the remainder after division",
        examples: &["=MOD(10, 3)", "=MOD(A1, 2)"],
    },
    FunctionInfo {
        name: "INT",
        category: FunctionCategory::Math,
        syntax: "INT(number)",
        description: "Rounds a number down to the nearest integer",
        examples: &["=INT(8.9)", "=INT(-8.9)"],
    },
    FunctionInfo {
        name: "TRUNC",
        category: FunctionCategory::Math,
        syntax: "TRUNC(number, [digits])",
        description: "Truncates a number toward zero",
        examples: &["=TRUNC(-2.9)", "=TRUNC(2.99, 1)"],
    },
    FunctionInfo {
        name: "CEILING",
        category: FunctionCategory::Math,
        syntax: "CEILING(number, [significance])",
        description: "Rounds away from zero to a multiple of significance. Mixed signs return #NUM!",
        examples: &["=CEILING(2.5, 1)", "=CEILING(4.2, 0.5)"],
    },
    FunctionInfo {
        name: "FLOOR",
        category: FunctionCategory::Math,
        syntax: "FLOOR(number, [significance])",
        description: "Rounds toward zero to a multiple of significance. Mixed signs return #NUM!",
        examples: &["=FLOOR(2.5, 1)", "=FLOOR(4.8, 0.5)"],
    },
    FunctionInfo {
        name: "SIGN",
        category: FunctionCategory::Math,
        syntax: "SIGN(number)",
        description: "Returns the sign of a number: 1 if positive, -1 if negative, 0 if zero",
        examples: &["=SIGN(10)", "=SIGN(-5)", "=SIGN(0)"],
    },
    FunctionInfo {
        name: "PI",
        category: FunctionCategory::Math,
        syntax: "PI()",
        description: "Returns the value of Pi (3.14159...)",
        examples: &["=PI()", "=PI()*A1^2"],
    },
    FunctionInfo {
        name: "EXP",
        category: FunctionCategory::Math,
        syntax: "EXP(number)",
        description: "Returns e raised to the power of a number",
        examples: &["=EXP(1)", "=EXP(A1)"],
    },
    FunctionInfo {
        name: "LN",
        category: FunctionCategory::Math,
        syntax: "LN(number)",
        description: "Returns the natural logarithm of a number",
        examples: &["=LN(2.71828)", "=LN(A1)"],
    },
    FunctionInfo {
        name: "LOG",
        category: FunctionCategory::Math,
        syntax: "LOG(number, [base])",
        description: "Returns the logarithm of a number to a specified base (default 10)",
        examples: &["=LOG(100)", "=LOG(8, 2)"],
    },
    FunctionInfo {
        name: "LOG10",
        category: FunctionCategory::Math,
        syntax: "LOG10(number)",
        description: "Returns the base-10 logarithm of a number",
        examples: &["=LOG10(100)", "=LOG10(A1)"],
    },
    FunctionInfo {
        name: "SIN",
        category: FunctionCategory::Math,
        syntax: "SIN(number)",
        description: "Returns the sine of an angle (in radians)",
        examples: &["=SIN(PI()/2)", "=SIN(A1)"],
    },
    FunctionInfo {
        name: "COS",
        category: FunctionCategory::Math,
        syntax: "COS(number)",
        description: "Returns the cosine of an angle (in radians)",
        examples: &["=COS(0)", "=COS(PI())"],
    },
    FunctionInfo {
        name: "TAN",
        category: FunctionCategory::Math,
        syntax: "TAN(number)",
        description: "Returns the tangent of an angle (in radians)",
        examples: &["=TAN(PI()/4)", "=TAN(A1)"],
    },
    FunctionInfo {
        name: "ASIN",
        category: FunctionCategory::Math,
        syntax: "ASIN(number)",
        description: "Returns the arcsine of a number (result in radians)",
        examples: &["=ASIN(0.5)", "=ASIN(A1)"],
    },
    FunctionInfo {
        name: "ACOS",
        category: FunctionCategory::Math,
        syntax: "ACOS(number)",
        description: "Returns the arccosine of a number (result in radians)",
        examples: &["=ACOS(0.5)", "=ACOS(A1)"],
    },
    FunctionInfo {
        name: "ATAN",
        category: FunctionCategory::Math,
        syntax: "ATAN(number)",
        description: "Returns the arctangent of a number (result in radians)",
        examples: &["=ATAN(1)", "=ATAN(A1)"],
    },
    FunctionInfo {
        name: "RAND",
        category: FunctionCategory::Math,
        syntax: "RAND()",
        description: "Returns a random number between 0 and 1",
        examples: &["=RAND()", "=RAND()*100"],
    },
    FunctionInfo {
        name: "RANDBETWEEN",
        category: FunctionCategory::Math,
        syntax: "RANDBETWEEN(bottom, top)",
        description: "Returns a random integer between two numbers",
        examples: &["=RANDBETWEEN(1, 100)", "=RANDBETWEEN(A1, B1)"],
    },
    FunctionInfo {
        name: "PRODUCT",
        category: FunctionCategory::Math,
        syntax: "PRODUCT(number1, [number2], ...)",
        description: "Multiplies all numbers in a range",
        examples: &["=PRODUCT(A1:A5)", "=PRODUCT(2, 3, 4)"],
    },
    FunctionInfo {
        name: "SUMPRODUCT",
        category: FunctionCategory::Math,
        syntax: "SUMPRODUCT(array1, [array2], ...)",
        description: "Multiplies corresponding range elements and returns the sum",
        examples: &["=SUMPRODUCT(A1:A3, B1:B3)"],
    },

    // ===== Statistical Functions =====
    FunctionInfo {
        name: "AVERAGE",
        category: FunctionCategory::Statistical,
        syntax: "AVERAGE(number1, [number2], ...)",
        description: "Returns the arithmetic mean of the arguments",
        examples: &["=AVERAGE(A1:A10)", "=AVERAGE(1, 2, 3, 4, 5)"],
    },
    FunctionInfo {
        name: "MIN",
        category: FunctionCategory::Statistical,
        syntax: "MIN(number1, [number2], ...)",
        description: "Returns the smallest value in a set of values",
        examples: &["=MIN(A1:A10)", "=MIN(1, 2, 3)"],
    },
    FunctionInfo {
        name: "MAX",
        category: FunctionCategory::Statistical,
        syntax: "MAX(number1, [number2], ...)",
        description: "Returns the largest value in a set of values",
        examples: &["=MAX(A1:A10)", "=MAX(1, 2, 3)"],
    },
    FunctionInfo {
        name: "COUNT",
        category: FunctionCategory::Statistical,
        syntax: "COUNT(value1, [value2], ...)",
        description: "Counts the number of cells containing numbers",
        examples: &["=COUNT(A1:A10)", "=COUNT(A1, B1, C1)"],
    },
    FunctionInfo {
        name: "COUNTA",
        category: FunctionCategory::Statistical,
        syntax: "COUNTA(value1, [value2], ...)",
        description: "Counts the number of non-empty cells",
        examples: &["=COUNTA(A1:A10)", "=COUNTA(A1, B1, C1)"],
    },
    FunctionInfo {
        name: "MEDIAN",
        category: FunctionCategory::Statistical,
        syntax: "MEDIAN(number1, [number2], ...)",
        description: "Returns the median (middle value) of the given numbers",
        examples: &["=MEDIAN(A1:A10)", "=MEDIAN(1, 2, 3, 4, 5)"],
    },
    FunctionInfo {
        name: "SUMIF",
        category: FunctionCategory::Statistical,
        syntax: "SUMIF(range, criteria, [sum_range])",
        description: "Sums cells that meet a specified condition",
        examples: &["=SUMIF(A1:A10, \">5\")", "=SUMIF(A1:A10, \"Apple\", B1:B10)"],
    },
    FunctionInfo {
        name: "COUNTIF",
        category: FunctionCategory::Statistical,
        syntax: "COUNTIF(range, criteria)",
        description: "Counts cells that meet a specified condition",
        examples: &["=COUNTIF(A1:A10, \">5\")", "=COUNTIF(A1:A10, \"Yes\")"],
    },
    FunctionInfo {
        name: "AVERAGEIF",
        category: FunctionCategory::Statistical,
        syntax: "AVERAGEIF(range, criteria, [average_range])",
        description: "Averages cells that meet a specified condition",
        examples: &["=AVERAGEIF(A1:A10, \">5\")", "=AVERAGEIF(A1:A10, \"X\", B1:B10)"],
    },
    FunctionInfo {
        name: "COUNTBLANK",
        category: FunctionCategory::Statistical,
        syntax: "COUNTBLANK(range)",
        description: "Counts empty cells in a range",
        examples: &["=COUNTBLANK(A1:A10)"],
    },
    FunctionInfo {
        name: "SUMIFS",
        category: FunctionCategory::Statistical,
        syntax: "SUMIFS(sum_range, criteria_range1, criteria1, ...)",
        description: "Sums cells that meet all specified conditions",
        examples: &["=SUMIFS(B1:B10, A1:A10, \"Apple\", C1:C10, \">5\")"],
    },
    FunctionInfo {
        name: "COUNTIFS",
        category: FunctionCategory::Statistical,
        syntax: "COUNTIFS(criteria_range1, criteria1, ...)",
        description: "Counts cells that meet all specified conditions",
        examples: &["=COUNTIFS(A1:A10, \"Yes\", B1:B10, \">0\")"],
    },
    FunctionInfo {
        name: "AVERAGEIFS",
        category: FunctionCategory::Statistical,
        syntax: "AVERAGEIFS(average_range, criteria_range1, criteria1, ...)",
        description: "Averages cells that meet all specified conditions",
        examples: &["=AVERAGEIFS(B1:B10, A1:A10, \"X\")"],
    },
    FunctionInfo {
        name: "STDEV",
        category: FunctionCategory::Statistical,
        syntax: "STDEV(number1, [number2], ...)",
        description: "Sample standard deviation (n-1). Same as STDEV.S",
        examples: &["=STDEV(A1:A10)", "=STDEV.S(A1:A10)"],
    },
    FunctionInfo {
        name: "STDEV.S",
        category: FunctionCategory::Statistical,
        syntax: "STDEV.S(number1, [number2], ...)",
        description: "Sample standard deviation (n-1)",
        examples: &["=STDEV.S(A1:A10)"],
    },
    FunctionInfo {
        name: "STDEVP",
        category: FunctionCategory::Statistical,
        syntax: "STDEVP(number1, [number2], ...)",
        description: "Population standard deviation. Same as STDEV.P",
        examples: &["=STDEVP(A1:A10)"],
    },
    FunctionInfo {
        name: "STDEV.P",
        category: FunctionCategory::Statistical,
        syntax: "STDEV.P(number1, [number2], ...)",
        description: "Population standard deviation",
        examples: &["=STDEV.P(A1:A10)"],
    },
    FunctionInfo {
        name: "VAR",
        category: FunctionCategory::Statistical,
        syntax: "VAR(number1, [number2], ...)",
        description: "Sample variance (n-1). Same as VAR.S",
        examples: &["=VAR(A1:A10)", "=VAR.S(A1:A10)"],
    },
    FunctionInfo {
        name: "VAR.S",
        category: FunctionCategory::Statistical,
        syntax: "VAR.S(number1, [number2], ...)",
        description: "Sample variance (n-1)",
        examples: &["=VAR.S(A1:A10)"],
    },
    FunctionInfo {
        name: "VARP",
        category: FunctionCategory::Statistical,
        syntax: "VARP(number1, [number2], ...)",
        description: "Population variance. Same as VAR.P",
        examples: &["=VARP(A1:A10)"],
    },
    FunctionInfo {
        name: "VAR.P",
        category: FunctionCategory::Statistical,
        syntax: "VAR.P(number1, [number2], ...)",
        description: "Population variance",
        examples: &["=VAR.P(A1:A10)"],
    },
    FunctionInfo {
        name: "LARGE",
        category: FunctionCategory::Statistical,
        syntax: "LARGE(array, k)",
        description: "Returns the k-th largest value in a range",
        examples: &["=LARGE(A1:A10, 2)"],
    },
    FunctionInfo {
        name: "SMALL",
        category: FunctionCategory::Statistical,
        syntax: "SMALL(array, k)",
        description: "Returns the k-th smallest value in a range",
        examples: &["=SMALL(A1:A10, 1)"],
    },

    // ===== Logical Functions =====
    FunctionInfo {
        name: "IF",
        category: FunctionCategory::Logical,
        syntax: "IF(condition, value_if_true, [value_if_false])",
        description: "Returns one value if a condition is true, another if false",
        examples: &["=IF(A1>10, \"Big\", \"Small\")", "=IF(A1=B1, \"Match\", \"No match\")"],
    },
    FunctionInfo {
        name: "AND",
        category: FunctionCategory::Logical,
        syntax: "AND(logical1, [logical2], ...)",
        description: "Returns TRUE if all arguments are true",
        examples: &["=AND(A1>0, B1>0)", "=AND(TRUE, TRUE, FALSE)"],
    },
    FunctionInfo {
        name: "OR",
        category: FunctionCategory::Logical,
        syntax: "OR(logical1, [logical2], ...)",
        description: "Returns TRUE if any argument is true",
        examples: &["=OR(A1>0, B1>0)", "=OR(FALSE, FALSE, TRUE)"],
    },
    FunctionInfo {
        name: "NOT",
        category: FunctionCategory::Logical,
        syntax: "NOT(logical)",
        description: "Reverses the logic of its argument",
        examples: &["=NOT(TRUE)", "=NOT(A1>10)"],
    },
    FunctionInfo {
        name: "XOR",
        category: FunctionCategory::Logical,
        syntax: "XOR(logical1, [logical2], ...)",
        description: "Returns TRUE if an odd number of arguments are true",
        examples: &["=XOR(TRUE, FALSE)", "=XOR(A1>0, B1>0, C1>0)"],
    },
    FunctionInfo {
        name: "TRUE",
        category: FunctionCategory::Logical,
        syntax: "TRUE()",
        description: "Returns the logical value TRUE",
        examples: &["=TRUE()"],
    },
    FunctionInfo {
        name: "FALSE",
        category: FunctionCategory::Logical,
        syntax: "FALSE()",
        description: "Returns the logical value FALSE",
        examples: &["=FALSE()"],
    },
    FunctionInfo {
        name: "IFERROR",
        category: FunctionCategory::Logical,
        syntax: "IFERROR(value, value_if_error)",
        description: "Returns value_if_error if value is an error, otherwise returns value",
        examples: &["=IFERROR(A1/B1, 0)", "=IFERROR(VLOOKUP(...), \"Not found\")"],
    },
    FunctionInfo {
        name: "IFNA",
        category: FunctionCategory::Logical,
        syntax: "IFNA(value, value_if_na)",
        description: "Returns value_if_na if value is #N/A, otherwise returns value",
        examples: &["=IFNA(VLOOKUP(...), \"Not found\")"],
    },
    FunctionInfo {
        name: "IFS",
        category: FunctionCategory::Logical,
        syntax: "IFS(condition1, value1, [condition2, value2], ...)",
        description: "Checks multiple conditions and returns the first true result",
        examples: &["=IFS(A1>90, \"A\", A1>80, \"B\", A1>70, \"C\", TRUE, \"F\")"],
    },
    FunctionInfo {
        name: "SWITCH",
        category: FunctionCategory::Logical,
        syntax: "SWITCH(expression, value1, result1, [value2, result2], ..., [default])",
        description: "Evaluates expression against a list of values and returns the matching result",
        examples: &["=SWITCH(A1, 1, \"One\", 2, \"Two\", \"Other\")"],
    },
    FunctionInfo {
        name: "CHOOSE",
        category: FunctionCategory::Logical,
        syntax: "CHOOSE(index, value1, [value2], ...)",
        description: "Returns the value at the specified index position",
        examples: &["=CHOOSE(2, \"A\", \"B\", \"C\")", "=CHOOSE(A1, 10, 20, 30)"],
    },

    // ===== Text Functions =====
    FunctionInfo {
        name: "LEN",
        category: FunctionCategory::Text,
        syntax: "LEN(text)",
        description: "Returns the number of characters in a text string",
        examples: &["=LEN(\"Hello\")", "=LEN(A1)"],
    },
    FunctionInfo {
        name: "UPPER",
        category: FunctionCategory::Text,
        syntax: "UPPER(text)",
        description: "Converts text to uppercase",
        examples: &["=UPPER(\"hello\")", "=UPPER(A1)"],
    },
    FunctionInfo {
        name: "LOWER",
        category: FunctionCategory::Text,
        syntax: "LOWER(text)",
        description: "Converts text to lowercase",
        examples: &["=LOWER(\"HELLO\")", "=LOWER(A1)"],
    },
    FunctionInfo {
        name: "PROPER",
        category: FunctionCategory::Text,
        syntax: "PROPER(text)",
        description: "Capitalizes the first letter of each word",
        examples: &["=PROPER(\"hello world\")", "=PROPER(A1)"],
    },
    FunctionInfo {
        name: "TRIM",
        category: FunctionCategory::Text,
        syntax: "TRIM(text)",
        description: "Removes leading and trailing spaces from text",
        examples: &["=TRIM(\"  hello  \")", "=TRIM(A1)"],
    },
    FunctionInfo {
        name: "LEFT",
        category: FunctionCategory::Text,
        syntax: "LEFT(text, [num_chars])",
        description: "Returns the leftmost characters from a text string",
        examples: &["=LEFT(\"Hello\", 2)", "=LEFT(A1, 3)"],
    },
    FunctionInfo {
        name: "RIGHT",
        category: FunctionCategory::Text,
        syntax: "RIGHT(text, [num_chars])",
        description: "Returns the rightmost characters from a text string",
        examples: &["=RIGHT(\"Hello\", 2)", "=RIGHT(A1, 3)"],
    },
    FunctionInfo {
        name: "MID",
        category: FunctionCategory::Text,
        syntax: "MID(text, start_num, num_chars)",
        description: "Returns characters from the middle of a text string",
        examples: &["=MID(\"Hello\", 2, 3)", "=MID(A1, 1, 5)"],
    },
    FunctionInfo {
        name: "CONCATENATE",
        category: FunctionCategory::Text,
        syntax: "CONCATENATE(text1, [text2], ...)",
        description: "Joins several text strings into one",
        examples: &["=CONCATENATE(A1, \" \", B1)", "=CONCATENATE(\"Hello\", \" \", \"World\")"],
    },
    FunctionInfo {
        name: "CONCAT",
        category: FunctionCategory::Text,
        syntax: "CONCAT(text1, [text2], ...)",
        description: "Joins several text strings into one (same as CONCATENATE)",
        examples: &["=CONCAT(A1, B1)", "=CONCAT(\"Hello\", \"World\")"],
    },
    FunctionInfo {
        name: "FIND",
        category: FunctionCategory::Text,
        syntax: "FIND(find_text, within_text, [start_num])",
        description: "Finds one text string within another (case-sensitive)",
        examples: &["=FIND(\"l\", \"Hello\")", "=FIND(A1, B1)"],
    },
    FunctionInfo {
        name: "SEARCH",
        category: FunctionCategory::Text,
        syntax: "SEARCH(find_text, within_text, [start_num])",
        description: "Finds one text string within another (case-insensitive)",
        examples: &["=SEARCH(\"L\", \"Hello\")", "=SEARCH(A1, B1)"],
    },
    FunctionInfo {
        name: "SUBSTITUTE",
        category: FunctionCategory::Text,
        syntax: "SUBSTITUTE(text, old_text, new_text, [instance_num])",
        description: "Replaces occurrences of old_text with new_text",
        examples: &["=SUBSTITUTE(A1, \"old\", \"new\")", "=SUBSTITUTE(\"aaa\", \"a\", \"b\", 2)"],
    },
    FunctionInfo {
        name: "REPLACE",
        category: FunctionCategory::Text,
        syntax: "REPLACE(old_text, start_num, num_chars, new_text)",
        description: "Replaces characters within text",
        examples: &["=REPLACE(\"Hello\", 1, 2, \"XX\")", "=REPLACE(A1, 1, 3, \"New\")"],
    },
    FunctionInfo {
        name: "REPT",
        category: FunctionCategory::Text,
        syntax: "REPT(text, number_times)",
        description: "Repeats text a specified number of times",
        examples: &["=REPT(\"*\", 10)", "=REPT(A1, 3)"],
    },
    FunctionInfo {
        name: "EXACT",
        category: FunctionCategory::Text,
        syntax: "EXACT(text1, text2)",
        description: "Checks if two text strings are exactly the same (case-sensitive)",
        examples: &["=EXACT(\"Hello\", \"hello\")", "=EXACT(A1, B1)"],
    },
    FunctionInfo {
        name: "VALUE",
        category: FunctionCategory::Text,
        syntax: "VALUE(text)",
        description: "Converts a text string to a number",
        examples: &["=VALUE(\"123\")", "=VALUE(A1)"],
    },
    FunctionInfo {
        name: "TEXT",
        category: FunctionCategory::Text,
        syntax: "TEXT(value, format_text)",
        description: "Converts a value to text using a number, percent, or date format",
        examples: &["=TEXT(1234.5, \"#,##0.00\")", "=TEXT(0.5, \"0%\")", "=TEXT(DATE(2024,8,18), \"yyyy-mm-dd\")"],
    },
    FunctionInfo {
        name: "CHAR",
        category: FunctionCategory::Text,
        syntax: "CHAR(number)",
        description: "Returns the character specified by a number (1-255)",
        examples: &["=CHAR(65)", "=CHAR(10)"],
    },
    FunctionInfo {
        name: "CODE",
        category: FunctionCategory::Text,
        syntax: "CODE(text)",
        description: "Returns the numeric code for the first character",
        examples: &["=CODE(\"A\")", "=CODE(A1)"],
    },

    // ===== Lookup Functions =====
    FunctionInfo {
        name: "VLOOKUP",
        category: FunctionCategory::Lookup,
        syntax: "VLOOKUP(lookup_value, table_array, col_index_num, [range_lookup])",
        description: "Looks up a value in the first column and returns a value in the same row",
        examples: &["=VLOOKUP(A1, B1:D10, 2, FALSE)", "=VLOOKUP(\"Apple\", A1:C100, 3, TRUE)"],
    },
    FunctionInfo {
        name: "HLOOKUP",
        category: FunctionCategory::Lookup,
        syntax: "HLOOKUP(lookup_value, table_array, row_index_num, [range_lookup])",
        description: "Looks up a value in the first row and returns a value in the same column",
        examples: &["=HLOOKUP(A1, A1:Z3, 2, FALSE)", "=HLOOKUP(\"Q1\", A1:D10, 3, TRUE)"],
    },
    FunctionInfo {
        name: "INDEX",
        category: FunctionCategory::Lookup,
        syntax: "INDEX(array, row_num, [column_num])",
        description: "Returns the value at a given position in a range",
        examples: &["=INDEX(A1:C10, 2, 3)", "=INDEX(A1:A10, 5)"],
    },
    FunctionInfo {
        name: "MATCH",
        category: FunctionCategory::Lookup,
        syntax: "MATCH(lookup_value, lookup_array, [match_type])",
        description: "Returns the relative position of an item in a range",
        examples: &["=MATCH(\"Apple\", A1:A10, 0)", "=MATCH(5, B1:B100, 1)"],
    },
    FunctionInfo {
        name: "ROW",
        category: FunctionCategory::Lookup,
        syntax: "ROW([reference])",
        description: "Returns the row number of a reference",
        examples: &["=ROW(A5)", "=ROW()"],
    },
    FunctionInfo {
        name: "COLUMN",
        category: FunctionCategory::Lookup,
        syntax: "COLUMN([reference])",
        description: "Returns the column number of a reference",
        examples: &["=COLUMN(C1)", "=COLUMN()"],
    },
    FunctionInfo {
        name: "ROWS",
        category: FunctionCategory::Lookup,
        syntax: "ROWS(array)",
        description: "Returns the number of rows in a reference",
        examples: &["=ROWS(A1:A10)", "=ROWS(A1:C100)"],
    },
    FunctionInfo {
        name: "COLUMNS",
        category: FunctionCategory::Lookup,
        syntax: "COLUMNS(array)",
        description: "Returns the number of columns in a reference",
        examples: &["=COLUMNS(A1:C1)", "=COLUMNS(A1:Z100)"],
    },

    // ===== Date & Time Functions =====
    FunctionInfo {
        name: "DATE",
        category: FunctionCategory::DateTime,
        syntax: "DATE(year, month, day)",
        description: "Creates a date from year, month, and day components",
        examples: &["=DATE(2024, 1, 15)", "=DATE(A1, B1, C1)"],
    },
    FunctionInfo {
        name: "TODAY",
        category: FunctionCategory::DateTime,
        syntax: "TODAY()",
        description: "Returns the current date",
        examples: &["=TODAY()", "=TODAY()+7"],
    },
    FunctionInfo {
        name: "NOW",
        category: FunctionCategory::DateTime,
        syntax: "NOW()",
        description: "Returns the current date and time",
        examples: &["=NOW()"],
    },
    FunctionInfo {
        name: "YEAR",
        category: FunctionCategory::DateTime,
        syntax: "YEAR(serial_number)",
        description: "Returns the year from a date",
        examples: &["=YEAR(TODAY())", "=YEAR(A1)"],
    },
    FunctionInfo {
        name: "MONTH",
        category: FunctionCategory::DateTime,
        syntax: "MONTH(serial_number)",
        description: "Returns the month from a date (1-12)",
        examples: &["=MONTH(TODAY())", "=MONTH(A1)"],
    },
    FunctionInfo {
        name: "DAY",
        category: FunctionCategory::DateTime,
        syntax: "DAY(serial_number)",
        description: "Returns the day from a date (1-31)",
        examples: &["=DAY(TODAY())", "=DAY(A1)"],
    },

    // ===== Information Functions =====
    FunctionInfo {
        name: "ISBLANK",
        category: FunctionCategory::Info,
        syntax: "ISBLANK(value)",
        description: "Returns TRUE if the cell is empty",
        examples: &["=ISBLANK(A1)"],
    },
    FunctionInfo {
        name: "ISERROR",
        category: FunctionCategory::Info,
        syntax: "ISERROR(value)",
        description: "Returns TRUE if the value is any error",
        examples: &["=ISERROR(A1/B1)", "=ISERROR(A1)"],
    },
    FunctionInfo {
        name: "ISNA",
        category: FunctionCategory::Info,
        syntax: "ISNA(value)",
        description: "Returns TRUE if the value is #N/A error",
        examples: &["=ISNA(VLOOKUP(...))", "=ISNA(A1)"],
    },
    FunctionInfo {
        name: "ISNUMBER",
        category: FunctionCategory::Info,
        syntax: "ISNUMBER(value)",
        description: "Returns TRUE if the value is a number",
        examples: &["=ISNUMBER(A1)", "=ISNUMBER(\"123\")"],
    },
    FunctionInfo {
        name: "ISTEXT",
        category: FunctionCategory::Info,
        syntax: "ISTEXT(value)",
        description: "Returns TRUE if the value is text",
        examples: &["=ISTEXT(A1)", "=ISTEXT(123)"],
    },
    FunctionInfo {
        name: "ISLOGICAL",
        category: FunctionCategory::Info,
        syntax: "ISLOGICAL(value)",
        description: "Returns TRUE if the value is a logical value (TRUE or FALSE)",
        examples: &["=ISLOGICAL(TRUE)", "=ISLOGICAL(A1)"],
    },
    FunctionInfo {
        name: "NA",
        category: FunctionCategory::Info,
        syntax: "NA()",
        description: "Returns the #N/A error value",
        examples: &["=NA()", "=IF(A1=\"\", NA(), A1)"],
    },
    FunctionInfo {
        name: "TYPE",
        category: FunctionCategory::Info,
        syntax: "TYPE(value)",
        description: "Returns a number indicating the type of value (1=number, 2=text, 4=logical, 16=error)",
        examples: &["=TYPE(A1)", "=TYPE(\"Hello\")"],
    },
    FunctionInfo {
        name: "N",
        category: FunctionCategory::Info,
        syntax: "N(value)",
        description: "Converts a value to a number",
        examples: &["=N(TRUE)", "=N(A1)"],
    },
];
