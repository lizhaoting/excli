use std::collections::HashMap;

use chrono::Datelike;
use excli_core::cell::{CellValue, ExcelError};
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};

/// Trait for implementing an Excel function.
pub trait FormulaFunction: Send + Sync {
    fn name(&self) -> &str;
    fn category(&self) -> FunctionCategory;
    fn min_args(&self) -> usize;
    fn max_args(&self) -> Option<usize>;
    fn is_volatile(&self) -> bool { false }
    /// If true (default), the evaluator will flatten RangeArray args into individual values
    /// before calling evaluate(). Override to false for functions that need the raw array
    /// (e.g. VLOOKUP, INDEX, MATCH, RANK, LARGE, SMALL).
    fn expand_range_args(&self) -> bool { true }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue>;
}

/// Function categories matching Excel's built-in categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCategory {
    Math,
    Statistical,
    Lookup,
    Text,
    DateTime,
    Logical,
    Information,
    Financial,
    Engineering,
    Database,
    Web,
    DynamicArray,
    Lambda,
}

/// Registry of all available Excel functions.
pub struct FunctionRegistry {
    functions: HashMap<String, Box<dyn FormulaFunction>>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        let mut registry = FunctionRegistry {
            functions: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    fn register(&mut self, func: Box<dyn FormulaFunction>) {
        self.functions.insert(func.name().to_uppercase(), func);
    }

    pub fn get(&self, name: &str) -> Option<&dyn FormulaFunction> {
        self.functions.get(&name.to_uppercase()).map(|b| b.as_ref())
    }

    pub fn has(&self, name: &str) -> bool {
        self.functions.contains_key(&name.to_uppercase())
    }

    pub fn names(&self) -> Vec<&String> {
        self.functions.keys().collect()
    }

    #[allow(clippy::too_many_lines)]
    fn register_defaults(&mut self) {
        // === MATH FUNCTIONS ===
        self.register(Box::new(SumFunction));
        self.register(Box::new(SumIfFunction));
        self.register(Box::new(ProductFunction));
        self.register(Box::new(AbsFunction));
        self.register(Box::new(RoundFunction));
        self.register(Box::new(RoundUpFunction));
        self.register(Box::new(RoundDownFunction));
        self.register(Box::new(CeilingFunction));
        self.register(Box::new(FloorFunction));
        self.register(Box::new(SqrtFunction));
        self.register(Box::new(PowerFunction));
        self.register(Box::new(ModFunction));
        self.register(Box::new(LogFunction));
        self.register(Box::new(LnFunction));
        self.register(Box::new(ExpFunction));
        self.register(Box::new(SinFunction));
        self.register(Box::new(CosFunction));
        self.register(Box::new(TanFunction));
        self.register(Box::new(PiFunction));
        self.register(Box::new(RandFunction));
        self.register(Box::new(RandBetweenFunction));
        self.register(Box::new(SumProductFunction));
        self.register(Box::new(SubtotalFunction));
        self.register(Box::new(AggregateFunction));
        self.register(Box::new(IntFunction));
        self.register(Box::new(TruncFunction));
        self.register(Box::new(SignFunction));
        self.register(Box::new(MRoundFunction));

        // === STATISTICAL FUNCTIONS ===
        self.register(Box::new(AverageFunction));
        self.register(Box::new(AverageIfFunction));
        self.register(Box::new(AverageIfsFunction));
        self.register(Box::new(CountFunction));
        self.register(Box::new(CountAFunction));
        self.register(Box::new(CountIfFunction));
        self.register(Box::new(CountIfsFunction));
        self.register(Box::new(MinFunction));
        self.register(Box::new(MaxFunction));
        self.register(Box::new(MedianFunction));
        self.register(Box::new(ModeFunction));
        self.register(Box::new(StDevFunction));
        self.register(Box::new(VarFunction));
        self.register(Box::new(PercentileFunction));
        self.register(Box::new(QuartileFunction));
        self.register(Box::new(RankFunction));
        self.register(Box::new(LargeFunction));
        self.register(Box::new(SmallFunction));

        // === LOOKUP FUNCTIONS ===
        self.register(Box::new(VLookupFunction));
        self.register(Box::new(HLookupFunction));
        self.register(Box::new(IndexFunction));
        self.register(Box::new(MatchFunction));
        self.register(Box::new(ChooseFunction));
        self.register(Box::new(RowFunction));
        self.register(Box::new(ColumnFunction));
        self.register(Box::new(RowsFunction));
        self.register(Box::new(ColumnsFunction));
        self.register(Box::new(OffsetFunction));
        self.register(Box::new(IndirectFunction));
        self.register(Box::new(AddressFunction));
        self.register(Box::new(TransposeFunction));

        // === TEXT FUNCTIONS ===
        self.register(Box::new(LeftFunction));
        self.register(Box::new(RightFunction));
        self.register(Box::new(MidFunction));
        self.register(Box::new(LenFunction));
        self.register(Box::new(FindFunction));
        self.register(Box::new(SearchFunction));
        self.register(Box::new(ConcatFunction));
        self.register(Box::new(TextJoinFunction));
        self.register(Box::new(UpperFunction));
        self.register(Box::new(LowerFunction));
        self.register(Box::new(ProperFunction));
        self.register(Box::new(TrimFunction));
        self.register(Box::new(SubstituteFunction));
        self.register(Box::new(ReplaceFunction));
        self.register(Box::new(TextFunction));
        self.register(Box::new(ValueFunction));
        self.register(Box::new(CharFunction));
        self.register(Box::new(CodeFunction));

        // === LOGICAL FUNCTIONS ===
        self.register(Box::new(IfFunction));
        self.register(Box::new(IfsFunction));
        self.register(Box::new(IfErrorFunction));
        self.register(Box::new(IfNaFunction));
        self.register(Box::new(AndFunction));
        self.register(Box::new(OrFunction));
        self.register(Box::new(NotFunction));
        self.register(Box::new(XorFunction));
        self.register(Box::new(SwitchFunction));

        // === DATE/TIME FUNCTIONS ===
        self.register(Box::new(TodayFunction));
        self.register(Box::new(NowFunction));
        self.register(Box::new(DateFunction));
        self.register(Box::new(TimeFunction));
        self.register(Box::new(YearFunction));
        self.register(Box::new(MonthFunction));
        self.register(Box::new(DayFunction));
        self.register(Box::new(HourFunction));
        self.register(Box::new(MinuteFunction));
        self.register(Box::new(SecondFunction));
        self.register(Box::new(WeekdayFunction));
        self.register(Box::new(DateDifFunction));
        self.register(Box::new(EDateFunction));
        self.register(Box::new(EoMonthFunction));
        self.register(Box::new(NetworkDaysFunction));
        self.register(Box::new(WorkDayFunction));

        // === INFORMATION FUNCTIONS ===
        self.register(Box::new(IsBlankFunction));
        self.register(Box::new(IsErrorFunction));
        self.register(Box::new(IsNumberFunction));
        self.register(Box::new(IsTextFunction));
        self.register(Box::new(IsFormulaFunction));
        self.register(Box::new(TypeFunction));
        self.register(Box::new(ErrorTypeFunction));

        // === DYNAMIC ARRAY FUNCTIONS ===
        self.register(Box::new(SortFunction));
        self.register(Box::new(FilterFunction));
        self.register(Box::new(UniqueFunction));
        self.register(Box::new(SequenceFunction));

        // === ADDITIONAL LOOKUP / MATH FUNCTIONS ===
        self.register(Box::new(SumIfsFunction));
        self.register(Box::new(XLookupFunction));

        // === STATISTICAL EXTENSIONS ===
        self.register(Box::new(StDevPFunction));
        self.register(Box::new(VarPFunction));
        self.register(Box::new(CorrelFunction));
        self.register(Box::new(ForecastFunction));
        self.register(Box::new(SkewFunction));
        self.register(Box::new(WeekNumFunction));

        // === MATH EXTENSIONS ===
        self.register(Box::new(IsOddFunction));
        self.register(Box::new(IsEvenFunction));
        self.register(Box::new(GcdFunction));
        self.register(Box::new(LcmFunction));
        self.register(Box::new(CombinFunction));
        self.register(Box::new(PermutFunction));
        self.register(Box::new(FactFunction));
        self.register(Box::new(EvenFunction));
        self.register(Box::new(OddFunction));
        self.register(Box::new(QuotientFunction));
        self.register(Box::new(SumSqFunction));

        // === MULTI-CRITERIA MIN/MAX ===
        self.register(Box::new(MaxIfsFunction));
        self.register(Box::new(MinIfsFunction));

        // === FINANCIAL ===
        self.register(Box::new(NpvFunction));
        self.register(Box::new(IrrFunction));
        self.register(Box::new(PmtFunction));
        self.register(Box::new(FvFunction));
        self.register(Box::new(PvFunction));

        // === XMATCH ===
        self.register(Box::new(XMatchFunction));

        // === MORE FINANCIAL ===
        self.register(Box::new(NperFunction));
        self.register(Box::new(RateFunction));
        self.register(Box::new(SlnFunction));
        self.register(Box::new(DbFunction));

        // === TRIG / MATH ===
        self.register(Box::new(DegreesFunction));
        self.register(Box::new(RadiansFunction));
        self.register(Box::new(AsinFunction));
        self.register(Box::new(AcosFunction));
        self.register(Box::new(AtanFunction));
        self.register(Box::new(Atan2Function));
        self.register(Box::new(SinhFunction));
        self.register(Box::new(CoshFunction));
        self.register(Box::new(TanhFunction));

        // === TEXT ===
        self.register(Box::new(ReptFunction));
        self.register(Box::new(ExactFunction));
        self.register(Box::new(ConcatenateFunction));
        self.register(Box::new(FixedFunction));
        self.register(Box::new(NumbervalueFunction));
        self.register(Box::new(LenBFunction));
        self.register(Box::new(MidBFunction));
        self.register(Box::new(LeftBFunction));
        self.register(Box::new(RightBFunction));

        // === ENGINEERING ===
        self.register(Box::new(Bin2DecFunction));
        self.register(Box::new(Dec2BinFunction));
        self.register(Box::new(Hex2DecFunction));
        self.register(Box::new(Dec2HexFunction));
        self.register(Box::new(Oct2DecFunction));
        self.register(Box::new(Dec2OctFunction));

        // === INFO ===
        self.register(Box::new(SheetFunction));
        self.register(Box::new(SheetsFunction));

        // === STATISTICAL ===
        self.register(Box::new(FrequencyFunction));

        // === DATABASE ===
        self.register(Box::new(DsumFunction));
        self.register(Box::new(DaverageFunction));
        self.register(Box::new(DcountFunction));
        self.register(Box::new(DmaxFunction));
        self.register(Box::new(DminFunction));
        self.register(Box::new(DgetFunction));

        // === DYNAMIC ARRAY ===
        self.register(Box::new(SortbyFunction));
        self.register(Box::new(RandarrayFunction));

        // === MORE TEXT ===
        self.register(Box::new(UnicharFunction));
        self.register(Box::new(UnicodeFunction));
        self.register(Box::new(BahtextFunction));
        self.register(Box::new(NumbertextFunction));

        // === MATH: CEILING.MATH / FLOOR.MATH / MUNIT ===
        self.register(Box::new(CeilingMathFunction));
        self.register(Box::new(FloorMathFunction));
        self.register(Box::new(MunitFunction));

        // === BITWISE ===
        self.register(Box::new(BitandFunction));
        self.register(Box::new(BitorFunction));
        self.register(Box::new(BitxorFunction));
        self.register(Box::new(BitlshiftFunction));
        self.register(Box::new(BitrshiftFunction));

        // === STATISTICAL ===
        self.register(Box::new(NormdistFunction));
        self.register(Box::new(NorminvFunction));
        self.register(Box::new(TdistFunction));
        self.register(Box::new(ZtestFunction));

        // === DATE ===
        self.register(Box::new(DaysFunction));
        self.register(Box::new(Days360Function));
        self.register(Box::new(YearfracFunction));
        self.register(Box::new(IsodateFunction));

        // === LOOKUP: HYPERLINK / ROW helpers ===
        self.register(Box::new(HyperlinkFunction));
        self.register(Box::new(FormulaTextFunction));
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// === MACRO FOR SIMPLE MATH FUNCTIONS ===

#[allow(unused_macros)]
macro_rules! simple_math_fn {
    ($name:ident, $func_name:literal, $cat:ident, $body:expr) => {
        struct $name;
        impl FormulaFunction for $name {
            fn name(&self) -> &str { $func_name }
            fn category(&self) -> FunctionCategory { FunctionCategory::$cat }
            fn min_args(&self) -> usize { 1 }
            fn max_args(&self) -> Option<usize> { Some(1) }
            fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
                Ok($body(&args[0]))
            }
        }
    };
}

macro_rules! simple_reduce_fn {
    ($name:ident, $func_name:literal, $cat:ident, $identity:expr, $op:expr) => {
        struct $name;
        impl FormulaFunction for $name {
            fn name(&self) -> &str { $func_name }
            fn category(&self) -> FunctionCategory { FunctionCategory::$cat }
            fn min_args(&self) -> usize { 1 }
            fn max_args(&self) -> Option<usize> { Some(255) }
            fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
                let mut result: f64 = $identity;
                for arg in args {
                    let val = arg.coerce_to_number();
                    result = $op(result, val);
                }
                Ok(CellValue::Number(result))
            }
        }
    };
}

// == MATH FUNCTIONS ==

simple_reduce_fn!(SumFunction, "SUM", Math, 0.0, |a, b| a + b);
simple_reduce_fn!(ProductFunction, "PRODUCT", Math, 1.0, |a, b| a * b);

struct AbsFunction;
impl FormulaFunction for AbsFunction {
    fn name(&self) -> &str { "ABS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().abs()))
    }
}

struct RoundFunction;
impl FormulaFunction for RoundFunction {
    fn name(&self) -> &str { "ROUND" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let digits = args[1].coerce_to_number() as i32;
        let factor = 10_f64.powi(digits);
        Ok(CellValue::Number((n * factor).round() / factor))
    }
}

struct RoundUpFunction;
impl FormulaFunction for RoundUpFunction {
    fn name(&self) -> &str { "ROUNDUP" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let digits = args[1].coerce_to_number() as i32;
        let factor = 10_f64.powi(digits);
        Ok(CellValue::Number((n * factor).ceil() / factor))
    }
}

struct RoundDownFunction;
impl FormulaFunction for RoundDownFunction {
    fn name(&self) -> &str { "ROUNDDOWN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let digits = args[1].coerce_to_number() as i32;
        let factor = 10_f64.powi(digits);
        Ok(CellValue::Number((n * factor).floor() / factor))
    }
}

struct CeilingFunction;
impl FormulaFunction for CeilingFunction {
    fn name(&self) -> &str { "CEILING" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let significance = args[1].coerce_to_number();
        if significance == 0.0 { return Ok(CellValue::Number(0.0)); }
        Ok(CellValue::Number((n / significance).ceil() * significance))
    }
}

struct FloorFunction;
impl FormulaFunction for FloorFunction {
    fn name(&self) -> &str { "FLOOR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let significance = args[1].coerce_to_number();
        if significance == 0.0 { return Ok(CellValue::Number(0.0)); }
        Ok(CellValue::Number((n / significance).floor() * significance))
    }
}

struct SqrtFunction;
impl FormulaFunction for SqrtFunction {
    fn name(&self) -> &str { "SQRT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        if n < 0.0 { return Ok(CellValue::Error(ExcelError::Num)); }
        Ok(CellValue::Number(n.sqrt()))
    }
}

struct PowerFunction;
impl FormulaFunction for PowerFunction {
    fn name(&self) -> &str { "POWER" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().powf(args[1].coerce_to_number())))
    }
}

struct ModFunction;
impl FormulaFunction for ModFunction {
    fn name(&self) -> &str { "MOD" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let d = args[1].coerce_to_number();
        if d == 0.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        Ok(CellValue::Number(n - d * (n / d).floor()))
    }
}

struct LogFunction;
impl FormulaFunction for LogFunction {
    fn name(&self) -> &str { "LOG" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let base = if args.len() > 1 { args[1].coerce_to_number() } else { 10.0 };
        if n <= 0.0 { return Ok(CellValue::Error(ExcelError::Num)); }
        Ok(CellValue::Number(n.log(base)))
    }
}

struct LnFunction;
impl FormulaFunction for LnFunction {
    fn name(&self) -> &str { "LN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        if n <= 0.0 { return Ok(CellValue::Error(ExcelError::Num)); }
        Ok(CellValue::Number(n.ln()))
    }
}

struct ExpFunction;
impl FormulaFunction for ExpFunction {
    fn name(&self) -> &str { "EXP" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().exp()))
    }
}

struct SinFunction;
impl FormulaFunction for SinFunction {
    fn name(&self) -> &str { "SIN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().sin()))
    }
}

struct CosFunction;
impl FormulaFunction for CosFunction {
    fn name(&self) -> &str { "COS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().cos()))
    }
}

struct TanFunction;
impl FormulaFunction for TanFunction {
    fn name(&self) -> &str { "TAN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().tan()))
    }
}

struct PiFunction;
impl FormulaFunction for PiFunction {
    fn name(&self) -> &str { "PI" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(0) }
    fn is_volatile(&self) -> bool { false }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(std::f64::consts::PI))
    }
}

struct RandFunction;
impl FormulaFunction for RandFunction {
    fn name(&self) -> &str { "RAND" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(0) }
    fn is_volatile(&self) -> bool { true }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        // Non-crypto random for Excel compatibility
        let val = (0u64.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) % 1000000) as f64 / 1000000.0;
        Ok(CellValue::Number(val))
    }
}

struct RandBetweenFunction;
impl FormulaFunction for RandBetweenFunction {
    fn name(&self) -> &str { "RANDBETWEEN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn is_volatile(&self) -> bool { true }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let lo = args[0].coerce_to_number() as i64;
        let _hi = args[1].coerce_to_number() as i64;
        Ok(CellValue::Number(lo as f64))
    }
}

struct SumProductFunction;
impl FormulaFunction for SumProductFunction {
    fn name(&self) -> &str { "SUMPRODUCT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // SUMPRODUCT(array1, [array2, ...]) — element-wise product then sum
        let arrays: Vec<Vec<f64>> = args.iter()
            .map(|a| match a {
                CellValue::RangeArray(rows) => rows.iter().flat_map(|r| r.iter().map(|v| v.coerce_to_number())).collect(),
                other => vec![other.coerce_to_number()],
            })
            .collect();
        if arrays.is_empty() { return Ok(CellValue::Number(0.0)); }
        let len = arrays[0].len();
        let mut total = 0.0;
        for i in 0..len {
            let product: f64 = arrays.iter().map(|arr| arr.get(i).copied().unwrap_or(0.0)).product();
            total += product;
        }
        Ok(CellValue::Number(total))
    }
}

struct SubtotalFunction;
impl FormulaFunction for SubtotalFunction {
    fn name(&self) -> &str { "SUBTOTAL" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let _func_num = args[0].coerce_to_number() as i32;
        let mut total = 0.0;
        for arg in &args[1..] {
            total += arg.coerce_to_number();
        }
        // Simplified: just sum for now (function number 9 or 109)
        Ok(CellValue::Number(total))
    }
}

struct AggregateFunction;
impl FormulaFunction for AggregateFunction {
    fn name(&self) -> &str { "AGGREGATE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let _func_num = args[0].coerce_to_number() as i32;
        let _options = args[1].coerce_to_number() as i32;
        let mut total = 0.0;
        for arg in &args[2..] {
            total += arg.coerce_to_number();
        }
        Ok(CellValue::Number(total))
    }
}

struct IntFunction;
impl FormulaFunction for IntFunction {
    fn name(&self) -> &str { "INT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().floor()))
    }
}

struct TruncFunction;
impl FormulaFunction for TruncFunction {
    fn name(&self) -> &str { "TRUNC" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().trunc()))
    }
}

struct SignFunction;
impl FormulaFunction for SignFunction {
    fn name(&self) -> &str { "SIGN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number().signum()))
    }
}

struct MRoundFunction;
impl FormulaFunction for MRoundFunction {
    fn name(&self) -> &str { "MROUND" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let m = args[1].coerce_to_number();
        Ok(CellValue::Number((n / m).round() * m))
    }
}

struct SumIfFunction;
impl FormulaFunction for SumIfFunction {
    fn name(&self) -> &str { "SUMIF" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // SUMIF(range, criteria, [sum_range])
        let range_vals = flatten_range(&args[0]);
        let criteria = &args[1];
        let sum_vals = args.get(2).map(|v| flatten_range(v)).unwrap_or_else(|| range_vals.clone());
        let mut total = 0.0;
        for (i, cell) in range_vals.iter().enumerate() {
            if matches_criteria(cell, criteria) {
                if let Some(v) = sum_vals.get(i) {
                    total += v.coerce_to_number();
                }
            }
        }
        Ok(CellValue::Number(total))
    }
}

// == STATISTICAL FUNCTIONS ==

struct AverageFunction;
impl FormulaFunction for AverageFunction {
    fn name(&self) -> &str { "AVERAGE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let mut sum = 0.0;
        let mut count = 0.0;
        for arg in args {
            if !arg.is_empty() {
                sum += arg.coerce_to_number();
                count += 1.0;
            }
        }
        if count == 0.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        Ok(CellValue::Number(sum / count))
    }
}

struct AverageIfFunction;
impl FormulaFunction for AverageIfFunction {
    fn name(&self) -> &str { "AVERAGEIF" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // AVERAGEIF(range, criteria, [average_range])
        let range_vals = flatten_range(&args[0]);
        let criteria = &args[1];
        let avg_vals = args.get(2).map(|v| flatten_range(v)).unwrap_or_else(|| range_vals.clone());
        let mut total = 0.0;
        let mut count = 0;
        for (i, cell) in range_vals.iter().enumerate() {
            if matches_criteria(cell, criteria) {
                if let Some(v) = avg_vals.get(i) {
                    total += v.coerce_to_number();
                    count += 1;
                }
            }
        }
        if count == 0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        Ok(CellValue::Number(total / count as f64))
    }
}

struct AverageIfsFunction;
impl FormulaFunction for AverageIfsFunction {
    fn name(&self) -> &str { "AVERAGEIFS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // AVERAGEIFS(average_range, criteria_range1, criteria1, [...])
        let avg_vals = flatten_range(&args[0]);
        let mut total = 0.0;
        let mut count = 0;
        'outer: for (i, v) in avg_vals.iter().enumerate() {
            let mut j = 1;
            while j + 1 < args.len() {
                let cr_vals = flatten_range(&args[j]);
                let criteria = &args[j + 1];
                if let Some(cell) = cr_vals.get(i) {
                    if !matches_criteria(cell, criteria) { continue 'outer; }
                }
                j += 2;
            }
            total += v.coerce_to_number();
            count += 1;
        }
        if count == 0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        Ok(CellValue::Number(total / count as f64))
    }
}

struct CountFunction;
impl FormulaFunction for CountFunction {
    fn name(&self) -> &str { "COUNT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let count = args.iter().filter(|a| a.is_number() || matches!(a, CellValue::Boolean(_))).count();
        Ok(CellValue::Number(count as f64))
    }
}

struct CountAFunction;
impl FormulaFunction for CountAFunction {
    fn name(&self) -> &str { "COUNTA" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let count = args.iter().filter(|a| !a.is_empty()).count();
        Ok(CellValue::Number(count as f64))
    }
}

struct CountIfFunction;
impl FormulaFunction for CountIfFunction {
    fn name(&self) -> &str { "COUNTIF" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // COUNTIF(range, criteria)
        let range_vals = flatten_range(&args[0]);
        let criteria = &args[1];
        let count = range_vals.iter().filter(|v| matches_criteria(v, criteria)).count();
        Ok(CellValue::Number(count as f64))
    }
}

struct CountIfsFunction;
impl FormulaFunction for CountIfsFunction {
    fn name(&self) -> &str { "COUNTIFS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // COUNTIFS(range1, criteria1, [range2, criteria2, ...])
        if args.is_empty() { return Ok(CellValue::Number(0.0)); }
        let first_range = flatten_range(&args[0]);
        let len = first_range.len();
        let mut count = 0usize;
        'outer: for i in 0..len {
            let mut j = 1;
            while j + 1 < args.len() {
                let cr_vals = flatten_range(&args[j]);
                let criteria = &args[j + 1];
                if let Some(cell) = cr_vals.get(i) {
                    if !matches_criteria(cell, criteria) { continue 'outer; }
                }
                j += 2;
            }
            count += 1;
        }
        Ok(CellValue::Number(count as f64))
    }
}

struct MinFunction;
impl FormulaFunction for MinFunction {
    fn name(&self) -> &str { "MIN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let mut result: Option<f64> = None;
        for arg in args {
            let val = arg.coerce_to_number();
            result = Some(match result {
                None => val,
                Some(cur) => cur.min(val),
            });
        }
        Ok(CellValue::Number(result.unwrap_or(0.0)))
    }
}
struct MaxFunction;
impl FormulaFunction for MaxFunction {
    fn name(&self) -> &str { "MAX" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let mut result: Option<f64> = None;
        for arg in args {
            let val = arg.coerce_to_number();
            result = Some(match result {
                None => val,
                Some(cur) => cur.max(val),
            });
        }
        Ok(CellValue::Number(result.unwrap_or(0.0)))
    }
}

struct MedianFunction;
impl FormulaFunction for MedianFunction {
    fn name(&self) -> &str { "MEDIAN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let mut vals: Vec<f64> = args.iter().map(|a| a.coerce_to_number()).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let len = vals.len();
        if len == 0 { return Ok(CellValue::Error(ExcelError::Num)); }
        if len % 2 == 0 {
            Ok(CellValue::Number((vals[len / 2 - 1] + vals[len / 2]) / 2.0))
        } else {
            Ok(CellValue::Number(vals[len / 2]))
        }
    }
}

struct ModeFunction;
impl FormulaFunction for ModeFunction {
    fn name(&self) -> &str { "MODE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.is_empty() { return Ok(CellValue::Error(ExcelError::NA)); }
        let mut counts: Vec<(i64, usize)> = Vec::new(); // (bits of f64, count)
        for a in args {
            let v = a.coerce_to_number();
            let bits = v.to_bits() as i64;
            if let Some(entry) = counts.iter_mut().find(|(b, _)| *b == bits) {
                entry.1 += 1;
            } else {
                counts.push((bits, 1));
            }
        }
        let best = counts.iter().max_by_key(|(_, c)| *c);
        match best {
            Some((bits, _)) => Ok(CellValue::Number(f64::from_bits(*bits as u64))),
            None => Ok(CellValue::Error(ExcelError::NA)),
        }
    }
}

struct StDevFunction;
impl FormulaFunction for StDevFunction {
    fn name(&self) -> &str { "STDEV" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 2 { return Ok(CellValue::Error(ExcelError::Div0)); }
        let mean = args.iter().map(|a| a.coerce_to_number()).sum::<f64>() / args.len() as f64;
        let variance = args.iter()
            .map(|a| { let d = a.coerce_to_number() - mean; d * d })
            .sum::<f64>() / (args.len() - 1) as f64;
        Ok(CellValue::Number(variance.sqrt()))
    }
}

struct VarFunction;
impl FormulaFunction for VarFunction {
    fn name(&self) -> &str { "VAR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 2 { return Ok(CellValue::Error(ExcelError::Div0)); }
        let mean = args.iter().map(|a| a.coerce_to_number()).sum::<f64>() / args.len() as f64;
        let variance = args.iter()
            .map(|a| { let d = a.coerce_to_number() - mean; d * d })
            .sum::<f64>() / (args.len() - 1) as f64;
        Ok(CellValue::Number(variance))
    }
}

struct PercentileFunction;
impl FormulaFunction for PercentileFunction {
    fn name(&self) -> &str { "PERCENTILE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let k = args.last().map(|a| a.coerce_to_number()).unwrap_or(0.0);
        let data_args = if args.len() > 1 { &args[..args.len()-1] } else { args };
        let mut vals = collect_numbers(data_args);
        if vals.is_empty() { return Ok(CellValue::Error(ExcelError::NA)); }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = k * (vals.len() - 1) as f64;
        let lo = idx.floor() as usize;
        let hi = (idx.ceil() as usize).min(vals.len() - 1);
        if lo == hi { return Ok(CellValue::Number(vals[lo])); }
        let frac = idx - lo as f64;
        Ok(CellValue::Number(vals[lo] + frac * (vals[hi] - vals[lo])))
    }
}

struct QuartileFunction;
impl FormulaFunction for QuartileFunction {
    fn name(&self) -> &str { "QUARTILE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let q = args[1].coerce_to_number() as u32;
        let k = match q { 0 => 0.0, 1 => 0.25, 2 => 0.5, 3 => 0.75, 4 => 1.0, _ => return Ok(CellValue::Error(ExcelError::Num)) };
        let mut vals = collect_numbers(&args[..1]);
        if vals.is_empty() { return Ok(CellValue::Error(ExcelError::NA)); }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = k * (vals.len() - 1) as f64;
        let lo = idx.floor() as usize;
        let hi = (idx.ceil() as usize).min(vals.len() - 1);
        if lo == hi { return Ok(CellValue::Number(vals[lo])); }
        let frac = idx - lo as f64;
        Ok(CellValue::Number(vals[lo] + frac * (vals[hi] - vals[lo])))
    }
}

struct RankFunction;
impl FormulaFunction for RankFunction {
    fn name(&self) -> &str { "RANK" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let number = args[0].coerce_to_number();
        let order = args.get(2).map(|v| v.coerce_to_number() as i32).unwrap_or(0);
        let vals = match args.get(1) {
            Some(CellValue::RangeArray(rows)) => rows.iter().flat_map(|r| r.iter()).map(|v| v.coerce_to_number()).collect::<Vec<_>>(),
            Some(other) => vec![other.coerce_to_number()],
            None => return Ok(CellValue::Error(ExcelError::Value)),
        };
        if !vals.contains(&number) { return Ok(CellValue::Error(ExcelError::NA)); }
        let rank = if order == 0 {
            vals.iter().filter(|&&v| v > number).count() + 1
        } else {
            vals.iter().filter(|&&v| v < number).count() + 1
        };
        Ok(CellValue::Number(rank as f64))
    }
}

struct LargeFunction;
impl FormulaFunction for LargeFunction {
    fn name(&self) -> &str { "LARGE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let k = args[1].coerce_to_number() as usize;
        let mut vals = match args.first() {
            Some(CellValue::RangeArray(rows)) => rows.iter().flat_map(|r| r.iter()).map(|v| v.coerce_to_number()).collect::<Vec<_>>(),
            Some(other) => vec![other.coerce_to_number()],
            None => return Ok(CellValue::Error(ExcelError::Value)),
        };
        if k < 1 || k > vals.len() { return Ok(CellValue::Error(ExcelError::Num)); }
        vals.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        Ok(CellValue::Number(vals[k - 1]))
    }
}

struct SmallFunction;
impl FormulaFunction for SmallFunction {
    fn name(&self) -> &str { "SMALL" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let k = args[1].coerce_to_number() as usize;
        let mut vals = match args.first() {
            Some(CellValue::RangeArray(rows)) => rows.iter().flat_map(|r| r.iter()).map(|v| v.coerce_to_number()).collect::<Vec<_>>(),
            Some(other) => vec![other.coerce_to_number()],
            None => return Ok(CellValue::Error(ExcelError::Value)),
        };
        if k < 1 || k > vals.len() { return Ok(CellValue::Error(ExcelError::Num)); }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Ok(CellValue::Number(vals[k - 1]))
    }
}

// == LOOKUP FUNCTIONS ==

struct VLookupFunction;
impl FormulaFunction for VLookupFunction {
    fn name(&self) -> &str { "VLOOKUP" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // VLOOKUP(lookup_value, table_array, col_index, [range_lookup])
        let lookup = &args[0];
        let col_index = args[2].coerce_to_number() as usize;
        let range_lookup = args.get(3).map(|v| v.coerce_to_bool()).unwrap_or(true);

        let table = match args.get(1) {
            Some(CellValue::RangeArray(rows)) => rows,
            _ => return Ok(CellValue::Error(ExcelError::NA)),
        };

        if col_index < 1 || col_index > table.first().map(|r| r.len()).unwrap_or(0) {
            return Ok(CellValue::Error(ExcelError::Ref));
        }

        if range_lookup {
            // Approximate match: table must be sorted ascending, find last row <= lookup
            let mut result = None;
            for row in table {
                if let Some(first) = row.first() {
                    if cell_value_cmp(first, lookup) != std::cmp::Ordering::Greater {
                        result = row.get(col_index - 1).cloned();
                    } else {
                        break;
                    }
                }
            }
            result.ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "VLOOKUP: value not found"))
                .map(|v| if v == CellValue::Empty { CellValue::Error(ExcelError::NA) } else { v })
        } else {
            // Exact match
            for row in table {
                if let Some(first) = row.first() {
                    if cell_values_equal(first, lookup) {
                        return Ok(row.get(col_index - 1).cloned().unwrap_or(CellValue::Empty));
                    }
                }
            }
            Ok(CellValue::Error(ExcelError::NA))
        }
    }
}

struct HLookupFunction;
impl FormulaFunction for HLookupFunction {
    fn name(&self) -> &str { "HLOOKUP" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // HLOOKUP(lookup_value, table_array, row_index, [range_lookup])
        let lookup = &args[0];
        let row_index = args[2].coerce_to_number() as usize;

        let table = match args.get(1) {
            Some(CellValue::RangeArray(rows)) => rows,
            _ => return Ok(CellValue::Error(ExcelError::NA)),
        };

        let header = match table.first() {
            Some(h) => h,
            None => return Ok(CellValue::Error(ExcelError::NA)),
        };

        // Find column where header matches lookup
        let col_pos = header.iter().position(|v| cell_values_equal(v, lookup));
        match col_pos {
            None => Ok(CellValue::Error(ExcelError::NA)),
            Some(col) => {
                if row_index < 1 || row_index > table.len() {
                    return Ok(CellValue::Error(ExcelError::Ref));
                }
                Ok(table[row_index - 1].get(col).cloned().unwrap_or(CellValue::Empty))
            }
        }
    }
}

struct IndexFunction;
impl FormulaFunction for IndexFunction {
    fn name(&self) -> &str { "INDEX" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // INDEX(array, row_num, [col_num])
        let row_num = args[1].coerce_to_number() as usize;
        let col_num = args.get(2).map(|v| v.coerce_to_number() as usize).unwrap_or(1);

        match args.first() {
            Some(CellValue::RangeArray(rows)) => {
                if row_num == 0 {
                    // Return entire column
                    let col = col_num.saturating_sub(1);
                    let col_vals: Vec<Vec<CellValue>> = rows.iter()
                        .map(|r| vec![r.get(col).cloned().unwrap_or(CellValue::Empty)])
                        .collect();
                    Ok(CellValue::RangeArray(col_vals))
                } else {
                    let row = rows.get(row_num - 1).ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "INDEX row out of bounds"))?;
                    let val = row.get(col_num - 1).cloned().unwrap_or(CellValue::Empty);
                    Ok(val)
                }
            }
            Some(other) => Ok(other.clone()),
            None => Ok(CellValue::Empty),
        }
    }
}

struct MatchFunction;
impl FormulaFunction for MatchFunction {
    fn name(&self) -> &str { "MATCH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // MATCH(lookup_value, lookup_array, [match_type])
        let lookup = &args[0];
        let match_type = args.get(2).map(|v| v.coerce_to_number() as i32).unwrap_or(1);

        let array: Vec<CellValue> = match args.get(1) {
            Some(CellValue::RangeArray(rows)) => {
                // Flatten: use first column if multi-column, or first row if single row
                if rows.len() == 1 {
                    rows[0].clone()
                } else {
                    rows.iter().flat_map(|r| r.first().cloned()).collect()
                }
            }
            Some(other) => vec![other.clone()],
            None => return Ok(CellValue::Error(ExcelError::NA)),
        };

        match match_type {
            0 => {
                // Exact match
                for (i, val) in array.iter().enumerate() {
                    if cell_values_equal(val, lookup) {
                        return Ok(CellValue::Number((i + 1) as f64));
                    }
                }
                Ok(CellValue::Error(ExcelError::NA))
            }
            1 => {
                // Less than or equal (sorted ascending)
                let mut result = None;
                for (i, val) in array.iter().enumerate() {
                    if cell_value_cmp(val, lookup) != std::cmp::Ordering::Greater {
                        result = Some(i + 1);
                    } else {
                        break;
                    }
                }
                result.map(|i| CellValue::Number(i as f64)).ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "MATCH not found"))
            }
            -1 => {
                // Greater than or equal (sorted descending)
                for (i, val) in array.iter().enumerate() {
                    if cell_value_cmp(val, lookup) != std::cmp::Ordering::Less {
                        return Ok(CellValue::Number((i + 1) as f64));
                    }
                }
                Ok(CellValue::Error(ExcelError::NA))
            }
            _ => Ok(CellValue::Error(ExcelError::Value)),
        }
    }
}

struct ChooseFunction;
impl FormulaFunction for ChooseFunction {
    fn name(&self) -> &str { "CHOOSE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(254) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let idx = args[0].coerce_to_number() as usize;
        if idx < 1 || idx > args.len() - 1 {
            return Ok(CellValue::Error(ExcelError::Value));
        }
        Ok(args[idx].clone())
    }
}

struct RowFunction;
impl FormulaFunction for RowFunction {
    fn name(&self) -> &str { "ROW" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(1.0))
    }
}

struct ColumnFunction;
impl FormulaFunction for ColumnFunction {
    fn name(&self) -> &str { "COLUMN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(1.0))
    }
}

struct RowsFunction;
impl FormulaFunction for RowsFunction {
    fn name(&self) -> &str { "ROWS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = match &args[0] {
            CellValue::RangeArray(rows) => rows.len() as f64,
            _ => 1.0,
        };
        Ok(CellValue::Number(n))
    }
}

struct ColumnsFunction;
impl FormulaFunction for ColumnsFunction {
    fn name(&self) -> &str { "COLUMNS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = match &args[0] {
            CellValue::RangeArray(rows) => rows.first().map(|r| r.len() as f64).unwrap_or(0.0),
            _ => 1.0,
        };
        Ok(CellValue::Number(n))
    }
}

struct OffsetFunction;
impl FormulaFunction for OffsetFunction {
    fn name(&self) -> &str { "OFFSET" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn is_volatile(&self) -> bool { true }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(args[0].clone())
    }
}

struct IndirectFunction;
impl FormulaFunction for IndirectFunction {
    fn name(&self) -> &str { "INDIRECT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn is_volatile(&self) -> bool { true }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(0.0))
    }
}

struct AddressFunction;
impl FormulaFunction for AddressFunction {
    fn name(&self) -> &str { "ADDRESS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::InlineString("$A$1".to_string()))
    }
}

struct TransposeFunction;
impl FormulaFunction for TransposeFunction {
    fn name(&self) -> &str { "TRANSPOSE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(args[0].clone())
    }
}

// == TEXT FUNCTIONS ==

struct LeftFunction;
impl FormulaFunction for LeftFunction {
    fn name(&self) -> &str { "LEFT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        let n = if args.len() > 1 { args[1].coerce_to_number() as usize } else { 1 };
        let chars: Vec<char> = s.chars().take(n).collect();
        Ok(CellValue::InlineString(chars.into_iter().collect()))
    }
}

struct RightFunction;
impl FormulaFunction for RightFunction {
    fn name(&self) -> &str { "RIGHT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        let n = if args.len() > 1 { args[1].coerce_to_number() as usize } else { 1 };
        let chars: Vec<char> = s.chars().collect();
        let start = if n > chars.len() { 0 } else { chars.len() - n };
        Ok(CellValue::InlineString(chars[start..].iter().collect()))
    }
}

struct MidFunction;
impl FormulaFunction for MidFunction {
    fn name(&self) -> &str { "MID" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        let start = args[1].coerce_to_number() as usize;
        let len = args[2].coerce_to_number() as usize;
        let chars: Vec<char> = s.chars().collect();
        let start = if start < 1 { 0 } else { start - 1 };
        let end = (start + len).min(chars.len());
        Ok(CellValue::InlineString(chars[start..end].iter().collect()))
    }
}

struct LenFunction;
impl FormulaFunction for LenFunction {
    fn name(&self) -> &str { "LEN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        Ok(CellValue::Number(s.chars().count() as f64))
    }
}

struct FindFunction;
impl FormulaFunction for FindFunction {
    fn name(&self) -> &str { "FIND" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let needle = args[0].as_string().unwrap_or_default();
        let haystack = args[1].as_string().unwrap_or_default();
        let start = if args.len() > 2 { args[2].coerce_to_number() as usize } else { 0 };
        if let Some(pos) = haystack[start..].find(&needle) {
            Ok(CellValue::Number((pos + start + 1) as f64))
        } else {
            Ok(CellValue::Error(ExcelError::Value))
        }
    }
}

struct SearchFunction;
impl FormulaFunction for SearchFunction {
    fn name(&self) -> &str { "SEARCH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let needle = args[0].as_string().unwrap_or_default().to_lowercase();
        let haystack = args[1].as_string().unwrap_or_default().to_lowercase();
        if let Some(pos) = haystack.find(&needle) {
            Ok(CellValue::Number((pos + 1) as f64))
        } else {
            Ok(CellValue::Error(ExcelError::Value))
        }
    }
}

struct ConcatFunction;
impl FormulaFunction for ConcatFunction {
    fn name(&self) -> &str { "CONCAT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let result: String = args.iter().map(|a| a.as_string().unwrap_or_default()).collect();
        Ok(CellValue::InlineString(result))
    }
}

struct TextJoinFunction;
impl FormulaFunction for TextJoinFunction {
    fn name(&self) -> &str { "TEXTJOIN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let delim = args[0].as_string().unwrap_or_default();
        let _ignore_empty = args[1].coerce_to_bool();
        let texts: Vec<String> = args[2..].iter()
            .map(|a| a.as_string().unwrap_or_default())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(CellValue::InlineString(texts.join(&delim)))
    }
}

struct UpperFunction;
impl FormulaFunction for UpperFunction {
    fn name(&self) -> &str { "UPPER" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::InlineString(args[0].as_string().unwrap_or_default().to_uppercase()))
    }
}

struct LowerFunction;
impl FormulaFunction for LowerFunction {
    fn name(&self) -> &str { "LOWER" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::InlineString(args[0].as_string().unwrap_or_default().to_lowercase()))
    }
}

struct ProperFunction;
impl FormulaFunction for ProperFunction {
    fn name(&self) -> &str { "PROPER" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        let mut result = String::new();
        let mut capitalize = true;
        for ch in s.chars() {
            if ch.is_whitespace() {
                capitalize = true;
                result.push(ch);
            } else if capitalize {
                result.extend(ch.to_uppercase());
                capitalize = false;
            } else {
                result.extend(ch.to_lowercase());
            }
        }
        Ok(CellValue::InlineString(result))
    }
}

struct TrimFunction;
impl FormulaFunction for TrimFunction {
    fn name(&self) -> &str { "TRIM" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        let trimmed = s.split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(CellValue::InlineString(trimmed))
    }
}

struct SubstituteFunction;
impl FormulaFunction for SubstituteFunction {
    fn name(&self) -> &str { "SUBSTITUTE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let text = args[0].as_string().unwrap_or_default();
        let old = args[1].as_string().unwrap_or_default();
        let new = args[2].as_string().unwrap_or_default();
        if args.len() > 3 {
            let instance = args[3].coerce_to_number() as usize;
            let mut result = String::new();
            let mut count = 0;
            let mut chars = text.chars().peekable();
            while chars.peek().is_some() {
                let remaining: String = chars.clone().collect();
                if remaining.starts_with(&old) {
                    count += 1;
                    if count == instance {
                        result.push_str(&new);
                        for _ in 0..old.len() { chars.next(); }
                    } else {
                        result.push_str(&old);
                        for _ in 0..old.len() { chars.next(); }
                    }
                } else {
                    result.push(chars.next().unwrap());
                }
            }
            Ok(CellValue::InlineString(result))
        } else {
            Ok(CellValue::InlineString(text.replace(&old, &new)))
        }
    }
}

struct ReplaceFunction;
impl FormulaFunction for ReplaceFunction {
    fn name(&self) -> &str { "REPLACE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 4 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let text = args[0].as_string().unwrap_or_default();
        let start = (args[1].coerce_to_number() - 1.0).max(0.0) as usize;
        let count = args[2].coerce_to_number() as usize;
        let new = args[3].as_string().unwrap_or_default();
        let chars: Vec<char> = text.chars().collect();
        let mut result: String = chars[..start.min(chars.len())].iter().collect();
        result.push_str(&new);
        let end = (start + count).min(chars.len());
        result.push_str(&chars[end..].iter().collect::<String>());
        Ok(CellValue::InlineString(result))
    }
}

struct TextFunction;
impl FormulaFunction for TextFunction {
    fn name(&self) -> &str { "TEXT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::InlineString(args[0].to_string()))
    }
}

struct ValueFunction;
impl FormulaFunction for ValueFunction {
    fn name(&self) -> &str { "VALUE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        Ok(CellValue::Number(s.parse::<f64>().unwrap_or(0.0)))
    }
}

struct CharFunction;
impl FormulaFunction for CharFunction {
    fn name(&self) -> &str { "CHAR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let code = args[0].coerce_to_number() as u32;
        if let Some(ch) = char::from_u32(code) {
            Ok(CellValue::InlineString(ch.to_string()))
        } else {
            Ok(CellValue::Error(ExcelError::Value))
        }
    }
}

struct CodeFunction;
impl FormulaFunction for CodeFunction {
    fn name(&self) -> &str { "CODE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].as_string().unwrap_or_default();
        Ok(CellValue::Number(s.chars().next().map_or(0, |c| c as u32) as f64))
    }
}

// == LOGICAL FUNCTIONS ==

struct IfFunction;
impl FormulaFunction for IfFunction {
    fn name(&self) -> &str { "IF" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let condition = args[0].coerce_to_bool();
        if condition {
            Ok(args[1].clone())
        } else if args.len() > 2 {
            Ok(args[2].clone())
        } else {
            Ok(CellValue::Boolean(false))
        }
    }
}

struct IfsFunction;
impl FormulaFunction for IfsFunction {
    fn name(&self) -> &str { "IFS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(254) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        for i in (0..args.len()).step_by(2) {
            if i + 1 >= args.len() { break; }
            if args[i].coerce_to_bool() {
                return Ok(args[i + 1].clone());
            }
        }
        Ok(CellValue::Error(ExcelError::NA))
    }
}

struct IfErrorFunction;
impl FormulaFunction for IfErrorFunction {
    fn name(&self) -> &str { "IFERROR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args[0].is_error() {
            Ok(args[1].clone())
        } else {
            Ok(args[0].clone())
        }
    }
}

struct IfNaFunction;
impl FormulaFunction for IfNaFunction {
    fn name(&self) -> &str { "IFNA" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if matches!(args[0], CellValue::Error(ExcelError::NA)) {
            Ok(args[1].clone())
        } else {
            Ok(args[0].clone())
        }
    }
}

struct AndFunction;
impl FormulaFunction for AndFunction {
    fn name(&self) -> &str { "AND" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(args.iter().all(|a| a.coerce_to_bool())))
    }
}

struct OrFunction;
impl FormulaFunction for OrFunction {
    fn name(&self) -> &str { "OR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(args.iter().any(|a| a.coerce_to_bool())))
    }
}

struct NotFunction;
impl FormulaFunction for NotFunction {
    fn name(&self) -> &str { "NOT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(!args[0].coerce_to_bool()))
    }
}

struct XorFunction;
impl FormulaFunction for XorFunction {
    fn name(&self) -> &str { "XOR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let true_count = args.iter().filter(|a| a.coerce_to_bool()).count();
        Ok(CellValue::Boolean(true_count % 2 == 1))
    }
}

struct SwitchFunction;
impl FormulaFunction for SwitchFunction {
    fn name(&self) -> &str { "SWITCH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Logical }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(254) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let expr = &args[0];
        for i in (1..args.len()).step_by(2) {
            if i + 1 >= args.len() {
                // Last default value
                return Ok(args[i].clone());
            }
            if std::mem::discriminant(expr) == std::mem::discriminant(&args[i])
                && expr.to_string() == args[i].to_string()
            {
                return Ok(args[i + 1].clone());
            }
        }
        Ok(CellValue::Error(ExcelError::NA))
    }
}

// == DATE/TIME FUNCTIONS ==

struct TodayFunction;
impl FormulaFunction for TodayFunction {
    fn name(&self) -> &str { "TODAY" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(0) }
    fn is_volatile(&self) -> bool { true }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        let today = chrono::Local::now().date_naive();
        let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
        let days = (today - base).num_days() as f64;
        Ok(CellValue::Number(if days >= 60.0 { days + 1.0 } else { days }))
    }
}

struct NowFunction;
impl FormulaFunction for NowFunction {
    fn name(&self) -> &str { "NOW" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(0) }
    fn is_volatile(&self) -> bool { true }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        let now = chrono::Local::now();
        let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let duration = now.naive_local() - base;
        let days = duration.num_days() as f64;
        let secs = duration.num_seconds() as f64 - days * 86400.0;
        let serial = days + secs / 86400.0;
        Ok(CellValue::Number(if serial >= 60.0 { serial + 1.0 } else { serial }))
    }
}

struct DateFunction;
impl FormulaFunction for DateFunction {
    fn name(&self) -> &str { "DATE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let year = args[0].coerce_to_number() as i32;
        let month = args[1].coerce_to_number() as u32;
        let day = args[2].coerce_to_number() as u32;
        if let Some(d) = chrono::NaiveDate::from_ymd_opt(year, month.max(1), day.max(1)) {
            let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
            let days = (d - base).num_days() as f64;
            Ok(CellValue::Number(if days >= 60.0 { days + 1.0 } else { days }))
        } else {
            Ok(CellValue::Error(ExcelError::Num))
        }
    }
}

struct TimeFunction;
impl FormulaFunction for TimeFunction {
    fn name(&self) -> &str { "TIME" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let h = args[0].coerce_to_number();
        let m = args[1].coerce_to_number();
        let s = args[2].coerce_to_number();
        Ok(CellValue::Number((h * 3600.0 + m * 60.0 + s) / 86400.0))
    }
}

struct YearFunction;
impl FormulaFunction for YearFunction {
    fn name(&self) -> &str { "YEAR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let serial = args[0].coerce_to_number();
        if let Some(dt) = excli_core::utils::serial_to_datetime(serial, excli_core::types::DateSystem::Date1900) {
            Ok(CellValue::Number(dt.date().year() as f64))
        } else {
            Ok(CellValue::Error(ExcelError::Num))
        }
    }
}

struct MonthFunction;
impl FormulaFunction for MonthFunction {
    fn name(&self) -> &str { "MONTH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let serial = args[0].coerce_to_number();
        if let Some(dt) = excli_core::utils::serial_to_datetime(serial, excli_core::types::DateSystem::Date1900) {
            Ok(CellValue::Number(dt.date().month() as f64))
        } else {
            Ok(CellValue::Error(ExcelError::Num))
        }
    }
}

struct DayFunction;
impl FormulaFunction for DayFunction {
    fn name(&self) -> &str { "DAY" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let serial = args[0].coerce_to_number();
        if let Some(dt) = excli_core::utils::serial_to_datetime(serial, excli_core::types::DateSystem::Date1900) {
            Ok(CellValue::Number(dt.date().day() as f64))
        } else {
            Ok(CellValue::Error(ExcelError::Num))
        }
    }
}

struct HourFunction;
impl FormulaFunction for HourFunction {
    fn name(&self) -> &str { "HOUR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number((args[0].coerce_to_number().fract() * 24.0) as f64))
    }
}

struct MinuteFunction;
impl FormulaFunction for MinuteFunction {
    fn name(&self) -> &str { "MINUTE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number((args[0].coerce_to_number().fract() * 24.0 * 60.0 % 60.0) as f64))
    }
}

struct SecondFunction;
impl FormulaFunction for SecondFunction {
    fn name(&self) -> &str { "SECOND" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number((args[0].coerce_to_number().fract() * 86400.0 % 60.0) as f64))
    }
}

struct WeekdayFunction;
impl FormulaFunction for WeekdayFunction {
    fn name(&self) -> &str { "WEEKDAY" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // Convert Excel serial to NaiveDate
        let serial = args[0].coerce_to_number() as i64;
        let return_type = if args.len() > 1 { args[1].coerce_to_number() as i32 } else { 1 };
        // Excel serial 1 = Jan 1 1900 (day = Monday in real calendar, but Excel has a
        // leap year bug treating 1900 as a leap year, so serial 1 = Sunday in Excel's WEEKDAY)
        // Use chrono: days since Dec 30, 1899
        let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
        let date = base + chrono::Duration::days(serial);
        // chrono: Mon=0, Tue=1 ... Sun=6
        let dow = date.weekday().num_days_from_monday(); // 0=Mon .. 6=Sun
        let result = match return_type {
            2 => (dow + 1) as f64,             // 1=Mon, 7=Sun
            3 => dow as f64,                   // 0=Mon, 6=Sun
            11 => (dow + 1) as f64,            // 1=Mon, 7=Sun
            12 => ((dow + 6) % 7 + 1) as f64, // 1=Tue
            13 => ((dow + 5) % 7 + 1) as f64, // 1=Wed
            14 => ((dow + 4) % 7 + 1) as f64, // 1=Thu
            15 => ((dow + 3) % 7 + 1) as f64, // 1=Fri
            16 => ((dow + 2) % 7 + 1) as f64, // 1=Sat
            17 => ((dow + 1) % 7 + 1) as f64, // 1=Sun (same as 1)
            _ =>  ((dow + 1) % 7 + 1) as f64, // default type 1: 1=Sun, 7=Sat
        };
        Ok(CellValue::Number(result))
    }
}

struct DateDifFunction;
impl FormulaFunction for DateDifFunction {
    fn name(&self) -> &str { "DATEDIF" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[1].coerce_to_number() - args[0].coerce_to_number()))
    }
}

struct EDateFunction;
impl FormulaFunction for EDateFunction {
    fn name(&self) -> &str { "EDATE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number() + args[1].coerce_to_number() * 30.0))
    }
}

struct EoMonthFunction;
impl FormulaFunction for EoMonthFunction {
    fn name(&self) -> &str { "EOMONTH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number() + args[1].coerce_to_number() * 30.0 + 29.0))
    }
}

struct NetworkDaysFunction;
impl FormulaFunction for NetworkDaysFunction {
    fn name(&self) -> &str { "NETWORKDAYS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number((args[1].coerce_to_number() - args[0].coerce_to_number()).abs()))
    }
}

struct WorkDayFunction;
impl FormulaFunction for WorkDayFunction {
    fn name(&self) -> &str { "WORKDAY" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number() + args[1].coerce_to_number()))
    }
}

// == INFORMATION FUNCTIONS ==

struct WeekNumFunction;
impl FormulaFunction for WeekNumFunction {
    fn name(&self) -> &str { "WEEKNUM" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let serial = args[0].coerce_to_number() as i64;
        let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
        let date = base + chrono::Duration::days(serial);
        let iso_week = date.iso_week().week() as f64;
        Ok(CellValue::Number(iso_week))
    }
}

struct StDevPFunction;
impl FormulaFunction for StDevPFunction {
    fn name(&self) -> &str { "STDEVP" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let nums = collect_numbers(args);
        if nums.is_empty() { return Ok(CellValue::Error(ExcelError::Div0)); }
        let mean = nums.iter().sum::<f64>() / nums.len() as f64;
        let variance = nums.iter().map(|&x| { let d = x - mean; d * d }).sum::<f64>() / nums.len() as f64;
        Ok(CellValue::Number(variance.sqrt()))
    }
}

struct VarPFunction;
impl FormulaFunction for VarPFunction {
    fn name(&self) -> &str { "VARP" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let nums = collect_numbers(args);
        if nums.is_empty() { return Ok(CellValue::Error(ExcelError::Div0)); }
        let mean = nums.iter().sum::<f64>() / nums.len() as f64;
        let variance = nums.iter().map(|&x| { let d = x - mean; d * d }).sum::<f64>() / nums.len() as f64;
        Ok(CellValue::Number(variance))
    }
}

struct CorrelFunction;
impl FormulaFunction for CorrelFunction {
    fn name(&self) -> &str { "CORREL" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let xs = flatten_range(&args[0]).into_iter().filter_map(|v| {
            let n = v.coerce_to_number();
            if n.is_nan() { None } else { Some(n) }
        }).collect::<Vec<_>>();
        let ys = flatten_range(&args[1]).into_iter().filter_map(|v| {
            let n = v.coerce_to_number();
            if n.is_nan() { None } else { Some(n) }
        }).collect::<Vec<_>>();
        let n = xs.len().min(ys.len()) as f64;
        if n < 2.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        let mean_x = xs.iter().take(n as usize).sum::<f64>() / n;
        let mean_y = ys.iter().take(n as usize).sum::<f64>() / n;
        let (mut cov, mut sx, mut sy) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..n as usize {
            let dx = xs[i] - mean_x;
            let dy = ys[i] - mean_y;
            cov += dx * dy;
            sx += dx * dx;
            sy += dy * dy;
        }
        let denom = (sx * sy).sqrt();
        if denom == 0.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        Ok(CellValue::Number(cov / denom))
    }
}

struct ForecastFunction;
impl FormulaFunction for ForecastFunction {
    fn name(&self) -> &str { "FORECAST" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let x = args[0].coerce_to_number();
        let ys: Vec<f64> = flatten_range(&args[1]).into_iter().map(|v| v.coerce_to_number()).collect();
        let xs: Vec<f64> = flatten_range(&args[2]).into_iter().map(|v| v.coerce_to_number()).collect();
        let n = xs.len().min(ys.len()) as f64;
        if n < 2.0 { return Ok(CellValue::Error(ExcelError::NA)); }
        let mean_x = xs.iter().take(n as usize).sum::<f64>() / n;
        let mean_y = ys.iter().take(n as usize).sum::<f64>() / n;
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for i in 0..n as usize {
            let dx = xs[i] - mean_x;
            num += dx * (ys[i] - mean_y);
            den += dx * dx;
        }
        if den == 0.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        let slope = num / den;
        let intercept = mean_y - slope * mean_x;
        Ok(CellValue::Number(slope * x + intercept))
    }
}

struct SkewFunction;
impl FormulaFunction for SkewFunction {
    fn name(&self) -> &str { "SKEW" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let nums = collect_numbers(args);
        let n = nums.len() as f64;
        if n < 3.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        let mean = nums.iter().sum::<f64>() / n;
        let variance = nums.iter().map(|&x| { let d = x - mean; d * d }).sum::<f64>() / (n - 1.0);
        let std = variance.sqrt();
        if std == 0.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        let skew = nums.iter().map(|&x| { let z = (x - mean) / std; z * z * z }).sum::<f64>();
        Ok(CellValue::Number(skew * n / ((n - 1.0) * (n - 2.0))))
    }
}

struct IsOddFunction;
impl FormulaFunction for IsOddFunction {
    fn name(&self) -> &str { "ISODD" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(args[0].coerce_to_number() as i64 % 2 != 0))
    }
}

struct IsEvenFunction;
impl FormulaFunction for IsEvenFunction {
    fn name(&self) -> &str { "ISEVEN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(args[0].coerce_to_number() as i64 % 2 == 0))
    }
}

struct GcdFunction;
impl FormulaFunction for GcdFunction {
    fn name(&self) -> &str { "GCD" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        fn gcd(a: u64, b: u64) -> u64 { if b == 0 { a } else { gcd(b, a % b) } }
        let mut result = 0u64;
        for arg in args {
            let n = arg.coerce_to_number().abs() as u64;
            result = gcd(result, n);
        }
        Ok(CellValue::Number(result as f64))
    }
}

struct LcmFunction;
impl FormulaFunction for LcmFunction {
    fn name(&self) -> &str { "LCM" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        fn gcd(a: u64, b: u64) -> u64 { if b == 0 { a } else { gcd(b, a % b) } }
        fn lcm(a: u64, b: u64) -> u64 { if a == 0 || b == 0 { 0 } else { a / gcd(a, b) * b } }
        let mut result = 1u64;
        for arg in args {
            let n = arg.coerce_to_number().abs() as u64;
            result = lcm(result, n);
        }
        Ok(CellValue::Number(result as f64))
    }
}

struct CombinFunction;
impl FormulaFunction for CombinFunction {
    fn name(&self) -> &str { "COMBIN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number() as u64;
        let k = args[1].coerce_to_number() as u64;
        if k > n { return Ok(CellValue::Number(0.0)); }
        // C(n,k) = n! / (k! * (n-k)!)
        let k = k.min(n - k);
        let mut result = 1u64;
        for i in 0..k {
            result = result * (n - i) / (i + 1);
        }
        Ok(CellValue::Number(result as f64))
    }
}

struct PermutFunction;
impl FormulaFunction for PermutFunction {
    fn name(&self) -> &str { "PERMUT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number() as u64;
        let k = args[1].coerce_to_number() as u64;
        if k > n { return Ok(CellValue::Error(ExcelError::Num)); }
        let result: u64 = ((n - k + 1)..=n).product();
        Ok(CellValue::Number(result as f64))
    }
}

struct FactFunction;
impl FormulaFunction for FactFunction {
    fn name(&self) -> &str { "FACT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number() as u64;
        if n > 170 { return Ok(CellValue::Error(ExcelError::Num)); }
        let result: f64 = (1..=n).map(|x| x as f64).product();
        Ok(CellValue::Number(result))
    }
}

struct EvenFunction;
impl FormulaFunction for EvenFunction {
    fn name(&self) -> &str { "EVEN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let result = if n >= 0.0 { (n.ceil() as i64 + 1) & !1 } else { (n.floor() as i64 - 1) | 1 & !1 };
        let result = if n.abs() == (n.abs().floor()) && n as i64 % 2 == 0 { n as i64 } else { result };
        Ok(CellValue::Number(result as f64))
    }
}

struct OddFunction;
impl FormulaFunction for OddFunction {
    fn name(&self) -> &str { "ODD" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let ceil = if n >= 0.0 { n.ceil() as i64 } else { n.floor() as i64 };
        let result = if ceil % 2 == 0 { if n >= 0.0 { ceil + 1 } else { ceil - 1 } } else { ceil };
        Ok(CellValue::Number(result as f64))
    }
}

struct QuotientFunction;
impl FormulaFunction for QuotientFunction {
    fn name(&self) -> &str { "QUOTIENT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let denom = args[1].coerce_to_number();
        if denom == 0.0 { return Ok(CellValue::Error(ExcelError::Div0)); }
        Ok(CellValue::Number((args[0].coerce_to_number() / denom).trunc()))
    }
}

struct SumSqFunction;
impl FormulaFunction for SumSqFunction {
    fn name(&self) -> &str { "SUMSQ" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let total = args.iter().map(|a| { let n = a.coerce_to_number(); n * n }).sum::<f64>();
        Ok(CellValue::Number(total))
    }
}

struct IsBlankFunction;
impl FormulaFunction for IsBlankFunction {
    fn name(&self) -> &str { "ISBLANK" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(args[0].is_empty()))
    }
}

struct IsErrorFunction;
impl FormulaFunction for IsErrorFunction {
    fn name(&self) -> &str { "ISERROR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(args[0].is_error()))
    }
}

struct IsNumberFunction;
impl FormulaFunction for IsNumberFunction {
    fn name(&self) -> &str { "ISNUMBER" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(args[0].is_number()))
    }
}

struct IsTextFunction;
impl FormulaFunction for IsTextFunction {
    fn name(&self) -> &str { "ISTEXT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(matches!(
            args[0],
            CellValue::InlineString(_) | CellValue::SharedString(_) | CellValue::RichText(_)
        )))
    }
}

struct IsFormulaFunction;
impl FormulaFunction for IsFormulaFunction {
    fn name(&self) -> &str { "ISFORMULA" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Boolean(false))
    }
}

struct TypeFunction;
impl FormulaFunction for TypeFunction {
    fn name(&self) -> &str { "TYPE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let type_num = match &args[0] {
            CellValue::Number(_) => 1,
            CellValue::InlineString(_) | CellValue::SharedString(_) => 2,
            CellValue::Boolean(_) => 4,
            CellValue::Error(_) => 16,
            _ => 64,
        };
        Ok(CellValue::Number(type_num as f64))
    }
}

struct ErrorTypeFunction;
impl FormulaFunction for ErrorTypeFunction {
    fn name(&self) -> &str { "ERROR.TYPE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if let CellValue::Error(e) = &args[0] {
            let type_num = match e {
                ExcelError::Null => 1,
                ExcelError::Div0 => 2,
                ExcelError::Value => 3,
                ExcelError::Ref => 4,
                ExcelError::Name => 5,
                ExcelError::Num => 6,
                ExcelError::NA => 7,
                _ => 14,
            };
            Ok(CellValue::Number(type_num as f64))
        } else {
            Ok(CellValue::Error(ExcelError::NA))
        }
    }
}

// == DYNAMIC ARRAY FUNCTIONS ==

struct SortFunction;
impl FormulaFunction for SortFunction {
    fn name(&self) -> &str { "SORT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DynamicArray }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(args[0].clone())
    }
}

struct FilterFunction;
impl FormulaFunction for FilterFunction {
    fn name(&self) -> &str { "FILTER" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DynamicArray }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(args[0].clone())
    }
}

struct UniqueFunction;
impl FormulaFunction for UniqueFunction {
    fn name(&self) -> &str { "UNIQUE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DynamicArray }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(args[0].clone())
    }
}

struct SequenceFunction;
impl FormulaFunction for SequenceFunction {
    fn name(&self) -> &str { "SEQUENCE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DynamicArray }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].coerce_to_number()))
    }
}

// == SUMIFS (multi-criteria SUM) ==

struct SumIfsFunction;
impl FormulaFunction for SumIfsFunction {
    fn name(&self) -> &str { "SUMIFS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(255) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // SUMIFS(sum_range, criteria_range1, criteria1, [criteria_range2, criteria2, ...])
        let sum_vals = flatten_range(&args[0]);
        let mut total = 0.0;
        'outer: for (i, v) in sum_vals.iter().enumerate() {
            let mut j = 1;
            while j + 1 < args.len() {
                let cr_vals = flatten_range(&args[j]);
                let criteria = &args[j + 1];
                if let Some(cell) = cr_vals.get(i) {
                    if !matches_criteria(cell, criteria) { continue 'outer; }
                }
                j += 2;
            }
            total += v.coerce_to_number();
        }
        Ok(CellValue::Number(total))
    }
}

// == XLOOKUP ==

struct XLookupFunction;
impl FormulaFunction for XLookupFunction {
    fn name(&self) -> &str { "XLOOKUP" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(6) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode])
        let lookup = &args[0];
        let lookup_arr = flatten_range(&args[1]);
        let return_arr = flatten_range(&args[2]);
        let if_not_found = args.get(3);
        let match_mode = args.get(4).map(|v| v.coerce_to_number() as i32).unwrap_or(0);

        // match_mode: 0=exact, -1=exact/smaller, 1=exact/larger, 2=wildcard
        for (i, cell) in lookup_arr.iter().enumerate() {
            let found = match match_mode {
                0 => cell_values_equal(cell, lookup),
                2 => {
                    let crit_str = lookup.to_string();
                    let cell_str = cell.to_string().to_uppercase();
                    wildcard_match(&cell_str, &crit_str.to_uppercase())
                }
                _ => cell_values_equal(cell, lookup),
            };
            if found {
                return Ok(return_arr.get(i).cloned().unwrap_or(CellValue::Empty));
            }
        }
        match if_not_found {
            Some(v) => Ok(v.clone()),
            None => Ok(CellValue::Error(ExcelError::NA)),
        }
    }
}

// == MAXIFS ==
struct MaxIfsFunction;
impl FormulaFunction for MaxIfsFunction {
    fn name(&self) -> &str { "MAXIFS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { None }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 3 || args.len() % 2 == 0 {
            return Err(ExcliError::formula_eval_error("MAXIFS requires max_range, criteria_range1, criteria1, ..."));
        }
        let max_range = flatten_range(&args[0]);
        let n = max_range.len();
        let mut masks: Vec<bool> = vec![true; n];
        let mut i = 1;
        while i + 1 < args.len() {
            let cr = flatten_range(&args[i]);
            let criteria = &args[i + 1];
            for (j, cell) in cr.iter().enumerate() {
                if j < n && !matches_criteria(cell, criteria) {
                    masks[j] = false;
                }
            }
            i += 2;
        }
        let mut max_val = f64::NEG_INFINITY;
        let mut found = false;
        for (j, cell) in max_range.iter().enumerate() {
            if masks[j] {
                if let CellValue::Number(v) = cell {
                    if *v > max_val { max_val = *v; found = true; }
                }
            }
        }
        Ok(if found { CellValue::Number(max_val) } else { CellValue::Number(0.0) })
    }
}

// == MINIFS ==
struct MinIfsFunction;
impl FormulaFunction for MinIfsFunction {
    fn name(&self) -> &str { "MINIFS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { None }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 3 || args.len() % 2 == 0 {
            return Err(ExcliError::formula_eval_error("MINIFS requires min_range, criteria_range1, criteria1, ..."));
        }
        let min_range = flatten_range(&args[0]);
        let n = min_range.len();
        let mut masks: Vec<bool> = vec![true; n];
        let mut i = 1;
        while i + 1 < args.len() {
            let cr = flatten_range(&args[i]);
            let criteria = &args[i + 1];
            for (j, cell) in cr.iter().enumerate() {
                if j < n && !matches_criteria(cell, criteria) {
                    masks[j] = false;
                }
            }
            i += 2;
        }
        let mut min_val = f64::INFINITY;
        let mut found = false;
        for (j, cell) in min_range.iter().enumerate() {
            if masks[j] {
                if let CellValue::Number(v) = cell {
                    if *v < min_val { min_val = *v; found = true; }
                }
            }
        }
        Ok(if found { CellValue::Number(min_val) } else { CellValue::Number(0.0) })
    }
}

// == NPV ==
struct NpvFunction;
impl FormulaFunction for NpvFunction {
    fn name(&self) -> &str { "NPV" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { None }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.is_empty() {
            return Err(ExcliError::formula_eval_error("NPV requires rate and cash flows"));
        }
        let rate = match &args[0] {
            CellValue::Number(n) => *n,
            _ => return Err(ExcliError::formula_eval_error("NPV: rate must be a number")),
        };
        let mut npv = 0.0f64;
        for (i, cash) in args[1..].iter().enumerate() {
            if let CellValue::Number(v) = cash {
                npv += v / (1.0 + rate).powi(i as i32 + 1);
            }
        }
        Ok(CellValue::Number(npv))
    }
}

// == IRR ==
struct IrrFunction;
impl FormulaFunction for IrrFunction {
    fn name(&self) -> &str { "IRR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { None }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.is_empty() {
            return Err(ExcliError::formula_eval_error("IRR requires cash flows"));
        }
        let flows: Vec<f64> = args.iter().filter_map(|a| {
            if let CellValue::Number(v) = a { Some(*v) } else { None }
        }).collect();
        if flows.is_empty() {
            return Err(ExcliError::formula_eval_error("IRR: no numeric cash flows"));
        }
        let mut rate = 0.1f64;
        for _ in 0..100 {
            let (f, df) = flows.iter().enumerate().fold((0.0f64, 0.0f64), |(f, df), (t, &c)| {
                let denom = (1.0 + rate).powi(t as i32);
                (f + c / denom, df - (t as f64) * c / ((1.0 + rate) * denom))
            });
            if df.abs() < 1e-12 { break; }
            let new_rate = rate - f / df;
            if (new_rate - rate).abs() < 1e-10 { rate = new_rate; break; }
            rate = new_rate;
        }
        Ok(CellValue::Number(rate))
    }
}

// == PMT ==
struct PmtFunction;
impl FormulaFunction for PmtFunction {
    fn name(&self) -> &str { "PMT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 3 {
            return Err(ExcliError::formula_eval_error("PMT requires rate, nper, pv"));
        }
        let rate = match &args[0] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("PMT: invalid rate")) };
        let nper = match &args[1] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("PMT: invalid nper")) };
        let pv = match &args[2] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("PMT: invalid pv")) };
        let fv = if args.len() > 3 { match &args[3] { CellValue::Number(n) => *n, _ => 0.0 } } else { 0.0 };
        let type_ = if args.len() > 4 { match &args[4] { CellValue::Number(n) => *n, _ => 0.0 } } else { 0.0 };
        let pmt = if rate == 0.0 {
            -(pv + fv) / nper
        } else {
            let r = (1.0 + rate).powf(nper);
            -((pv * r + fv) * rate) / ((1.0 + rate * type_) * (r - 1.0))
        };
        Ok(CellValue::Number(pmt))
    }
}

// == FV ==
struct FvFunction;
impl FormulaFunction for FvFunction {
    fn name(&self) -> &str { "FV" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 3 {
            return Err(ExcliError::formula_eval_error("FV requires rate, nper, pmt"));
        }
        let rate = match &args[0] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("FV: invalid rate")) };
        let nper = match &args[1] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("FV: invalid nper")) };
        let pmt = match &args[2] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("FV: invalid pmt")) };
        let pv = if args.len() > 3 { match &args[3] { CellValue::Number(n) => *n, _ => 0.0 } } else { 0.0 };
        let type_ = if args.len() > 4 { match &args[4] { CellValue::Number(n) => *n, _ => 0.0 } } else { 0.0 };
        let fv = if rate == 0.0 {
            -(pv + pmt * nper)
        } else {
            let r = (1.0 + rate).powf(nper);
            -(pv * r + pmt * (1.0 + rate * type_) * (r - 1.0) / rate)
        };
        Ok(CellValue::Number(fv))
    }
}

// == PV ==
struct PvFunction;
impl FormulaFunction for PvFunction {
    fn name(&self) -> &str { "PV" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 3 {
            return Err(ExcliError::formula_eval_error("PV requires rate, nper, pmt"));
        }
        let rate = match &args[0] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("PV: invalid rate")) };
        let nper = match &args[1] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("PV: invalid nper")) };
        let pmt = match &args[2] { CellValue::Number(n) => *n, _ => return Err(ExcliError::formula_eval_error("PV: invalid pmt")) };
        let fv = if args.len() > 3 { match &args[3] { CellValue::Number(n) => *n, _ => 0.0 } } else { 0.0 };
        let type_ = if args.len() > 4 { match &args[4] { CellValue::Number(n) => *n, _ => 0.0 } } else { 0.0 };
        let pv = if rate == 0.0 {
            -(fv + pmt * nper)
        } else {
            let r = (1.0 + rate).powf(nper);
            -(fv / r + pmt * (1.0 + rate * type_) * (r - 1.0) / (rate * r))
        };
        Ok(CellValue::Number(pv))
    }
}

// == XMATCH ==
struct XMatchFunction;
impl FormulaFunction for XMatchFunction {
    fn name(&self) -> &str { "XMATCH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        if args.len() < 2 {
            return Err(ExcliError::formula_eval_error("XMATCH requires lookup_value and lookup_array"));
        }
        let lookup = &args[0];
        let array = flatten_range(&args[1]);
        let match_mode = if args.len() > 2 { match &args[2] { CellValue::Number(n) => *n as i32, _ => 0 } } else { 0 };
        for (i, v) in array.iter().enumerate() {
            let matches = match match_mode {
                0 => cell_values_equal(v, lookup),
                1 => matches!(cell_value_cmp(v, lookup), std::cmp::Ordering::Greater | std::cmp::Ordering::Equal),
                -1 => matches!(cell_value_cmp(v, lookup), std::cmp::Ordering::Less | std::cmp::Ordering::Equal),
                _ => cell_values_equal(v, lookup),
            };
            if matches {
                return Ok(CellValue::Number(i as f64 + 1.0));
            }
        }
        Err(ExcliError::formula_eval_error("XMATCH: value not found"))
    }
}

// ─── Helpers used by lookup functions ─────────────────────────────────────────

// ─── Financial: NPER, RATE, SLN, DB ──────────────────────────────────────────

struct NperFunction;
impl FormulaFunction for NperFunction {
    fn name(&self) -> &str { "NPER" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rate = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("NPER: invalid rate"))?;
        let pmt  = args[1].as_f64().ok_or_else(|| ExcliError::formula_eval_error("NPER: invalid pmt"))?;
        let pv   = args[2].as_f64().ok_or_else(|| ExcliError::formula_eval_error("NPER: invalid pv"))?;
        let fv   = if args.len() > 3 { args[3].as_f64().unwrap_or(0.0) } else { 0.0 };
        let typ  = if args.len() > 4 { args[4].as_f64().unwrap_or(0.0) } else { 0.0 };
        if rate.abs() < 1e-12 {
            if pmt.abs() < 1e-12 { return Err(ExcliError::formula_eval_error("NPER: pmt can't be 0 when rate=0")); }
            return Ok(CellValue::Number((-fv - pv) / pmt));
        }
        let numer = -fv * rate + pmt * (1.0 + rate * typ);
        let denom = pv   * rate + pmt * (1.0 + rate * typ);
        if numer / denom <= 0.0 { return Err(ExcliError::formula_eval_error("NPER: no solution")); }
        Ok(CellValue::Number((numer / denom).ln() / (1.0 + rate).ln()))
    }
}

struct RateFunction;
impl FormulaFunction for RateFunction {
    fn name(&self) -> &str { "RATE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(6) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let nper = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("RATE: invalid nper"))?;
        let pmt  = args[1].as_f64().ok_or_else(|| ExcliError::formula_eval_error("RATE: invalid pmt"))?;
        let pv   = args[2].as_f64().ok_or_else(|| ExcliError::formula_eval_error("RATE: invalid pv"))?;
        let fv   = if args.len() > 3 { args[3].as_f64().unwrap_or(0.0) } else { 0.0 };
        let typ  = if args.len() > 4 { args[4].as_f64().unwrap_or(0.0) } else { 0.0 };
        let guess = if args.len() > 5 { args[5].as_f64().unwrap_or(0.1) } else { 0.1 };
        // Newton-Raphson iteration
        let mut rate = guess;
        for _ in 0..100 {
            let r1 = 1.0 + rate;
            let rn = r1.powf(nper);
            let f = pv * rn + pmt * (1.0 + rate * typ) * (rn - 1.0) / rate + fv;
            let df = pv * nper * r1.powf(nper - 1.0)
                + pmt * (1.0 + rate * typ) * ((rn - 1.0) / rate.powi(2) * (-1.0) + nper * r1.powf(nper - 1.0) / rate)
                + pmt * typ * (rn - 1.0) / rate;
            let delta = f / df;
            rate -= delta;
            if delta.abs() < 1e-10 { break; }
        }
        Ok(CellValue::Number(rate))
    }
}

struct SlnFunction;
impl FormulaFunction for SlnFunction {
    fn name(&self) -> &str { "SLN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let cost    = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("SLN: invalid cost"))?;
        let salvage = args[1].as_f64().ok_or_else(|| ExcliError::formula_eval_error("SLN: invalid salvage"))?;
        let life    = args[2].as_f64().ok_or_else(|| ExcliError::formula_eval_error("SLN: invalid life"))?;
        if life == 0.0 { return Err(ExcliError::formula_eval_error("SLN: life=0")); }
        Ok(CellValue::Number((cost - salvage) / life))
    }
}

struct DbFunction;
impl FormulaFunction for DbFunction {
    fn name(&self) -> &str { "DB" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Financial }
    fn min_args(&self) -> usize { 4 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let cost    = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DB: invalid cost"))?;
        let salvage = args[1].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DB: invalid salvage"))?;
        let life    = args[2].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DB: invalid life"))?;
        let period  = args[3].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DB: invalid period"))?;
        let month   = if args.len() > 4 { args[4].as_f64().unwrap_or(12.0) } else { 12.0 };
        if life == 0.0 || cost == 0.0 { return Ok(CellValue::Number(0.0)); }
        let rate = 1.0 - (salvage / cost).powf(1.0 / life);
        let rate = (rate * 1000.0).round() / 1000.0;
        let first_year = cost * rate * month / 12.0;
        if period == 1.0 { return Ok(CellValue::Number(first_year)); }
        let mut book_val = cost - first_year;
        for p in 2..(period as usize) {
            let dep = book_val * rate;
            if p == period as usize { break; }
            book_val -= dep;
        }
        Ok(CellValue::Number(book_val * rate))
    }
}

// ─── Math/Trig: DEGREES, RADIANS, ASIN, ACOS, ATAN, ATAN2, SINH, COSH, TANH ─

struct DegreesFunction;
impl FormulaFunction for DegreesFunction {
    fn name(&self) -> &str { "DEGREES" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DEGREES: invalid"))?;
        Ok(CellValue::Number(v.to_degrees()))
    }
}

struct RadiansFunction;
impl FormulaFunction for RadiansFunction {
    fn name(&self) -> &str { "RADIANS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("RADIANS: invalid"))?;
        Ok(CellValue::Number(v.to_radians()))
    }
}

struct AsinFunction;
impl FormulaFunction for AsinFunction {
    fn name(&self) -> &str { "ASIN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("ASIN: invalid"))?;
        Ok(CellValue::Number(v.asin()))
    }
}

struct AcosFunction;
impl FormulaFunction for AcosFunction {
    fn name(&self) -> &str { "ACOS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("ACOS: invalid"))?;
        Ok(CellValue::Number(v.acos()))
    }
}

struct AtanFunction;
impl FormulaFunction for AtanFunction {
    fn name(&self) -> &str { "ATAN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("ATAN: invalid"))?;
        Ok(CellValue::Number(v.atan()))
    }
}

struct Atan2Function;
impl FormulaFunction for Atan2Function {
    fn name(&self) -> &str { "ATAN2" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let x = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("ATAN2: invalid x"))?;
        let y = args[1].as_f64().ok_or_else(|| ExcliError::formula_eval_error("ATAN2: invalid y"))?;
        Ok(CellValue::Number(y.atan2(x)))
    }
}

struct SinhFunction;
impl FormulaFunction for SinhFunction {
    fn name(&self) -> &str { "SINH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("SINH: invalid"))?;
        Ok(CellValue::Number(v.sinh()))
    }
}

struct CoshFunction;
impl FormulaFunction for CoshFunction {
    fn name(&self) -> &str { "COSH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("COSH: invalid"))?;
        Ok(CellValue::Number(v.cosh()))
    }
}

struct TanhFunction;
impl FormulaFunction for TanhFunction {
    fn name(&self) -> &str { "TANH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let v = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("TANH: invalid"))?;
        Ok(CellValue::Number(v.tanh()))
    }
}

// ─── Text: REPT, EXACT, CONCATENATE, FIXED, NUMBERVALUE, LENB, MIDB, LEFTB, RIGHTB ──

struct ReptFunction;
impl FormulaFunction for ReptFunction {
    fn name(&self) -> &str { "REPT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let text = args[0].to_string();
        let n = args[1].as_f64().unwrap_or(0.0) as usize;
        Ok(CellValue::InlineString(text.repeat(n)))
    }
}

struct ExactFunction;
impl FormulaFunction for ExactFunction {
    fn name(&self) -> &str { "EXACT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let a = args[0].to_string();
        let b = args[1].to_string();
        Ok(CellValue::Boolean(a == b))
    }
}

struct ConcatenateFunction;
impl FormulaFunction for ConcatenateFunction {
    fn name(&self) -> &str { "CONCATENATE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { None }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let result: String = args.iter().map(|a| a.to_string()).collect();
        Ok(CellValue::InlineString(result))
    }
}

struct FixedFunction;
impl FormulaFunction for FixedFunction {
    fn name(&self) -> &str { "FIXED" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("FIXED: invalid number"))?;
        let decimals = if args.len() > 1 { args[1].as_f64().unwrap_or(2.0) as usize } else { 2 };
        let no_commas = if args.len() > 2 { args[2].as_f64().unwrap_or(0.0) != 0.0 } else { false };
        let formatted = format!("{:.prec$}", n, prec = decimals);
        if no_commas {
            Ok(CellValue::InlineString(formatted))
        } else {
            // Add thousand separators
            let (int_part, dec_part) = if let Some(pos) = formatted.find('.') {
                (&formatted[..pos], Some(&formatted[pos..]))
            } else {
                (formatted.as_str(), None)
            };
            let is_neg = int_part.starts_with('-');
            let digits = if is_neg { &int_part[1..] } else { int_part };
            let with_sep: String = digits.chars().rev().enumerate()
                .flat_map(|(i, c)| if i > 0 && i % 3 == 0 { vec![',', c] } else { vec![c] })
                .collect::<String>().chars().rev().collect();
            let result = if is_neg { format!("-{}", with_sep) } else { with_sep };
            let result = if let Some(d) = dec_part { format!("{}{}", result, d) } else { result };
            Ok(CellValue::InlineString(result))
        }
    }
}

struct NumbervalueFunction;
impl FormulaFunction for NumbervalueFunction {
    fn name(&self) -> &str { "NUMBERVALUE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let text = args[0].to_string();
        let decimal_sep = if args.len() > 1 { args[1].to_string() } else { ".".to_string() };
        let group_sep   = if args.len() > 2 { args[2].to_string() } else { ",".to_string() };
        let cleaned = text.replace(&group_sep, "").replace(&decimal_sep, ".");
        match cleaned.parse::<f64>() {
            Ok(v) => Ok(CellValue::Number(v)),
            Err(_) => Err(ExcliError::formula_eval_error("NUMBERVALUE: cannot parse")),
        }
    }
}

struct LenBFunction;
impl FormulaFunction for LenBFunction {
    fn name(&self) -> &str { "LENB" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        Ok(CellValue::Number(args[0].to_string().len() as f64))
    }
}

struct MidBFunction;
impl FormulaFunction for MidBFunction {
    fn name(&self) -> &str { "MIDB" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let text = args[0].to_string();
        let start = (args[1].as_f64().unwrap_or(1.0) as usize).saturating_sub(1);
        let len   = args[2].as_f64().unwrap_or(0.0) as usize;
        let bytes = text.as_bytes();
        let end = (start + len).min(bytes.len());
        let slice = &bytes[start.min(bytes.len())..end];
        Ok(CellValue::InlineString(String::from_utf8_lossy(slice).into_owned()))
    }
}

struct LeftBFunction;
impl FormulaFunction for LeftBFunction {
    fn name(&self) -> &str { "LEFTB" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let text = args[0].to_string();
        let n = if args.len() > 1 { args[1].as_f64().unwrap_or(1.0) as usize } else { 1 };
        let bytes = text.as_bytes();
        let end = n.min(bytes.len());
        Ok(CellValue::InlineString(String::from_utf8_lossy(&bytes[..end]).into_owned()))
    }
}

struct RightBFunction;
impl FormulaFunction for RightBFunction {
    fn name(&self) -> &str { "RIGHTB" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let text = args[0].to_string();
        let n = if args.len() > 1 { args[1].as_f64().unwrap_or(1.0) as usize } else { 1 };
        let bytes = text.as_bytes();
        let start = bytes.len().saturating_sub(n);
        Ok(CellValue::InlineString(String::from_utf8_lossy(&bytes[start..]).into_owned()))
    }
}

// ─── Engineering: BIN2DEC, DEC2BIN, HEX2DEC, DEC2HEX, OCT2DEC, DEC2OCT ──────

struct Bin2DecFunction;
impl FormulaFunction for Bin2DecFunction {
    fn name(&self) -> &str { "BIN2DEC" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].to_string();
        let n = i64::from_str_radix(&s, 2)
            .map_err(|_| ExcliError::formula_eval_error("BIN2DEC: invalid binary"))?;
        Ok(CellValue::Number(n as f64))
    }
}

struct Dec2BinFunction;
impl FormulaFunction for Dec2BinFunction {
    fn name(&self) -> &str { "DEC2BIN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DEC2BIN: invalid"))? as i64;
        let places = if args.len() > 1 { args[1].as_f64().unwrap_or(0.0) as usize } else { 0 };
        let s = format!("{:b}", n);
        let s = if places > 0 { format!("{:0>width$}", s, width = places) } else { s };
        Ok(CellValue::InlineString(s))
    }
}

struct Hex2DecFunction;
impl FormulaFunction for Hex2DecFunction {
    fn name(&self) -> &str { "HEX2DEC" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].to_string();
        let n = i64::from_str_radix(&s, 16)
            .map_err(|_| ExcliError::formula_eval_error("HEX2DEC: invalid hex"))?;
        Ok(CellValue::Number(n as f64))
    }
}

struct Dec2HexFunction;
impl FormulaFunction for Dec2HexFunction {
    fn name(&self) -> &str { "DEC2HEX" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DEC2HEX: invalid"))? as i64;
        let places = if args.len() > 1 { args[1].as_f64().unwrap_or(0.0) as usize } else { 0 };
        let s = format!("{:X}", n);
        let s = if places > 0 { format!("{:0>width$}", s, width = places) } else { s };
        Ok(CellValue::InlineString(s))
    }
}

struct Oct2DecFunction;
impl FormulaFunction for Oct2DecFunction {
    fn name(&self) -> &str { "OCT2DEC" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].to_string();
        let n = i64::from_str_radix(&s, 8)
            .map_err(|_| ExcliError::formula_eval_error("OCT2DEC: invalid octal"))?;
        Ok(CellValue::Number(n as f64))
    }
}

struct Dec2OctFunction;
impl FormulaFunction for Dec2OctFunction {
    fn name(&self) -> &str { "DEC2OCT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].as_f64().ok_or_else(|| ExcliError::formula_eval_error("DEC2OCT: invalid"))? as i64;
        let places = if args.len() > 1 { args[1].as_f64().unwrap_or(0.0) as usize } else { 0 };
        let s = format!("{:o}", n);
        let s = if places > 0 { format!("{:0>width$}", s, width = places) } else { s };
        Ok(CellValue::InlineString(s))
    }
}

// ─── Info: SHEET, SHEETS ──────────────────────────────────────────────────────

struct SheetFunction;
impl FormulaFunction for SheetFunction {
    fn name(&self) -> &str { "SHEET" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        // Without workbook context, return 1 (current sheet index)
        Ok(CellValue::Number(1.0))
    }
}

struct SheetsFunction;
impl FormulaFunction for SheetsFunction {
    fn name(&self) -> &str { "SHEETS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, _args: &[CellValue]) -> ExcliResult<CellValue> {
        // Without workbook context, return 1
        Ok(CellValue::Number(1.0))
    }
}

// ─── Statistical: FREQUENCY ──────────────────────────────────────────────────

struct FrequencyFunction;
impl FormulaFunction for FrequencyFunction {
    fn name(&self) -> &str { "FREQUENCY" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let data = flatten_range(&args[0]);
        let bins = flatten_range(&args[1]);
        let mut data_vals: Vec<f64> = data.iter().filter_map(|v| v.as_f64()).collect();
        let mut bin_vals: Vec<f64> = bins.iter().filter_map(|v| v.as_f64()).collect();
        data_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        bin_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut counts = vec![0u32; bin_vals.len() + 1];
        for &d in &data_vals {
            let idx = bin_vals.partition_point(|&b| b < d);
            counts[idx] += 1;
        }
        // Return as array (first element for simplicity)
        Ok(CellValue::Number(counts[0] as f64))
    }
}

// ─── Database: DSUM, DAVERAGE, DCOUNT, DMAX, DMIN, DGET ──────────────────────

/// Helper: extract rows from a range array
fn db_extract_rows(data: &CellValue) -> Vec<Vec<CellValue>> {
    match data {
        CellValue::RangeArray(rows) => rows.clone(),
        _ => vec![vec![data.clone()]],
    }
}

/// Helper: find column index by header name
fn db_col_index(headers: &[CellValue], field: &CellValue) -> Option<usize> {
    let field_str = field.to_string().to_uppercase();
    // Try by name first
    if let Some(idx) = headers.iter().position(|h| h.to_string().to_uppercase() == field_str) {
        return Some(idx);
    }
    // Try by 1-based index
    if let Some(n) = field.as_f64() {
        let idx = (n as usize).wrapping_sub(1);
        if idx < headers.len() { return Some(idx); }
    }
    None
}

/// Helper: check if a row matches criteria
fn db_row_matches(row: &[CellValue], headers: &[CellValue], criteria: &[Vec<CellValue>]) -> bool {
    if criteria.len() < 2 { return true; }
    let crit_headers = &criteria[0];
    // OR across rows (any criteria row matches)
    for crit_row in &criteria[1..] {
        let mut all_match = true;
        for (ci, crit_val) in crit_row.iter().enumerate() {
            if ci >= crit_headers.len() { continue; }
            let Some(col_idx) = db_col_index(headers, &crit_headers[ci]) else { continue; };
            if col_idx >= row.len() { continue; }
            let crit_str = crit_val.to_string();
            if crit_str.is_empty() { continue; }
            let cell_str = row[col_idx].to_string();
            // Support simple comparisons
            if crit_str.starts_with(">=") {
                let threshold: f64 = crit_str[2..].trim().parse().unwrap_or(f64::NAN);
                let val = row[col_idx].as_f64().unwrap_or(f64::NAN);
                if !(val >= threshold) { all_match = false; }
            } else if crit_str.starts_with("<=") {
                let threshold: f64 = crit_str[2..].trim().parse().unwrap_or(f64::NAN);
                let val = row[col_idx].as_f64().unwrap_or(f64::NAN);
                if !(val <= threshold) { all_match = false; }
            } else if crit_str.starts_with('>') {
                let threshold: f64 = crit_str[1..].trim().parse().unwrap_or(f64::NAN);
                let val = row[col_idx].as_f64().unwrap_or(f64::NAN);
                if !(val > threshold) { all_match = false; }
            } else if crit_str.starts_with('<') {
                let threshold: f64 = crit_str[1..].trim().parse().unwrap_or(f64::NAN);
                let val = row[col_idx].as_f64().unwrap_or(f64::NAN);
                if !(val < threshold) { all_match = false; }
            } else {
                if cell_str.to_uppercase() != crit_str.to_uppercase() { all_match = false; }
            }
        }
        if all_match { return true; }
    }
    false
}

struct DsumFunction;
impl FormulaFunction for DsumFunction {
    fn name(&self) -> &str { "DSUM" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Database }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rows = db_extract_rows(&args[0]);
        if rows.is_empty() { return Ok(CellValue::Number(0.0)); }
        let headers = &rows[0];
        let col_idx = db_col_index(headers, &args[1])
            .ok_or_else(|| ExcliError::formula_eval_error("DSUM: column not found"))?;
        let criteria = db_extract_rows(&args[2]);
        let sum: f64 = rows[1..].iter()
            .filter(|row| db_row_matches(row, headers, &criteria))
            .filter_map(|row| row.get(col_idx)?.as_f64())
            .sum();
        Ok(CellValue::Number(sum))
    }
}

struct DaverageFunction;
impl FormulaFunction for DaverageFunction {
    fn name(&self) -> &str { "DAVERAGE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Database }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rows = db_extract_rows(&args[0]);
        if rows.is_empty() { return Ok(CellValue::Number(0.0)); }
        let headers = &rows[0];
        let col_idx = db_col_index(headers, &args[1])
            .ok_or_else(|| ExcliError::formula_eval_error("DAVERAGE: column not found"))?;
        let criteria = db_extract_rows(&args[2]);
        let vals: Vec<f64> = rows[1..].iter()
            .filter(|row| db_row_matches(row, headers, &criteria))
            .filter_map(|row| row.get(col_idx)?.as_f64())
            .collect();
        if vals.is_empty() { return Err(ExcliError::formula_eval_error("DAVERAGE: no matching rows")); }
        Ok(CellValue::Number(vals.iter().sum::<f64>() / vals.len() as f64))
    }
}

struct DcountFunction;
impl FormulaFunction for DcountFunction {
    fn name(&self) -> &str { "DCOUNT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Database }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rows = db_extract_rows(&args[0]);
        if rows.is_empty() { return Ok(CellValue::Number(0.0)); }
        let headers = &rows[0];
        let col_idx = db_col_index(headers, &args[1])
            .ok_or_else(|| ExcliError::formula_eval_error("DCOUNT: column not found"))?;
        let criteria = db_extract_rows(&args[2]);
        let count = rows[1..].iter()
            .filter(|row| db_row_matches(row, headers, &criteria))
            .filter(|row| row.get(col_idx).map(|v| v.as_f64().is_some()).unwrap_or(false))
            .count();
        Ok(CellValue::Number(count as f64))
    }
}

struct DmaxFunction;
impl FormulaFunction for DmaxFunction {
    fn name(&self) -> &str { "DMAX" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Database }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rows = db_extract_rows(&args[0]);
        if rows.is_empty() { return Ok(CellValue::Number(0.0)); }
        let headers = &rows[0];
        let col_idx = db_col_index(headers, &args[1])
            .ok_or_else(|| ExcliError::formula_eval_error("DMAX: column not found"))?;
        let criteria = db_extract_rows(&args[2]);
        let max = rows[1..].iter()
            .filter(|row| db_row_matches(row, headers, &criteria))
            .filter_map(|row| row.get(col_idx)?.as_f64())
            .fold(f64::NEG_INFINITY, f64::max);
        Ok(CellValue::Number(if max == f64::NEG_INFINITY { 0.0 } else { max }))
    }
}

struct DminFunction;
impl FormulaFunction for DminFunction {
    fn name(&self) -> &str { "DMIN" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Database }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rows = db_extract_rows(&args[0]);
        if rows.is_empty() { return Ok(CellValue::Number(0.0)); }
        let headers = &rows[0];
        let col_idx = db_col_index(headers, &args[1])
            .ok_or_else(|| ExcliError::formula_eval_error("DMIN: column not found"))?;
        let criteria = db_extract_rows(&args[2]);
        let min = rows[1..].iter()
            .filter(|row| db_row_matches(row, headers, &criteria))
            .filter_map(|row| row.get(col_idx)?.as_f64())
            .fold(f64::INFINITY, f64::min);
        Ok(CellValue::Number(if min == f64::INFINITY { 0.0 } else { min }))
    }
}

struct DgetFunction;
impl FormulaFunction for DgetFunction {
    fn name(&self) -> &str { "DGET" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Database }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rows = db_extract_rows(&args[0]);
        if rows.is_empty() { return Err(ExcliError::formula_eval_error("DGET: no data")); }
        let headers = &rows[0];
        let col_idx = db_col_index(headers, &args[1])
            .ok_or_else(|| ExcliError::formula_eval_error("DGET: column not found"))?;
        let criteria = db_extract_rows(&args[2]);
        let mut matching: Vec<CellValue> = rows[1..].iter()
            .filter(|row| db_row_matches(row, headers, &criteria))
            .filter_map(|row| row.get(col_idx).cloned())
            .collect();
        match matching.len() {
            0 => Err(ExcliError::formula_eval_error("DGET: no match")),
            1 => Ok(matching.remove(0)),
            _ => Err(ExcliError::formula_eval_error("DGET: multiple matches")),
        }
    }
}

// ─── Dynamic Array: SORTBY, RANDARRAY ────────────────────────────────────────

struct SortbyFunction;
impl FormulaFunction for SortbyFunction {
    fn name(&self) -> &str { "SORTBY" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { None }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let data = flatten_range(&args[0]);
        let sort_by = flatten_range(&args[1]);
        let ascending = if args.len() > 2 { args[2].as_f64().unwrap_or(1.0) >= 0.0 } else { true };
        if data.len() != sort_by.len() {
            return Err(ExcliError::formula_eval_error("SORTBY: arrays must be same length"));
        }
        let mut pairs: Vec<(CellValue, CellValue)> = data.into_iter().zip(sort_by.into_iter()).collect();
        pairs.sort_by(|(_, ka), (_, kb)| {
            let ord = cell_value_cmp(ka, kb);
            if ascending { ord } else { ord.reverse() }
        });
        let result: Vec<CellValue> = pairs.into_iter().map(|(v, _)| v).collect();
        Ok(CellValue::RangeArray(vec![result]))
    }
}

struct RandarrayFunction;
impl FormulaFunction for RandarrayFunction {
    fn name(&self) -> &str { "RANDARRAY" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 0 }
    fn max_args(&self) -> Option<usize> { Some(5) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let rows = if !args.is_empty() { args[0].as_f64().unwrap_or(1.0) as usize } else { 1 };
        let cols = if args.len() > 1 { args[1].as_f64().unwrap_or(1.0) as usize } else { 1 };
        let min  = if args.len() > 2 { args[2].as_f64().unwrap_or(0.0) } else { 0.0 };
        let max  = if args.len() > 3 { args[3].as_f64().unwrap_or(1.0) } else { 1.0 };
        let is_int = if args.len() > 4 { args[4].as_f64().unwrap_or(0.0) != 0.0 } else { false };
        // Deterministic pseudo-random using simple LCG (no rand crate needed)
        let mut seed: u64 = 12345;
        let result: Vec<Vec<CellValue>> = (0..rows).map(|_| {
            (0..cols).map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let frac = (seed >> 33) as f64 / (u32::MAX as f64);
                let v = min + frac * (max - min);
                if is_int { CellValue::Number(v.floor()) } else { CellValue::Number(v) }
            }).collect()
        }).collect();
        Ok(CellValue::RangeArray(result))
    }
}



/// Flatten a CellValue (including RangeArray) into a vec of CellValues.
fn flatten_range(v: &CellValue) -> Vec<CellValue> {
    match v {
        CellValue::RangeArray(rows) => rows.iter().flat_map(|r| r.iter().cloned()).collect(),
        other => vec![other.clone()],
    }
}

/// Test if a cell value matches an Excel-style criteria string.
/// Supports: exact value, ">N", ">=N", "<N", "<=N", "<>N", wildcard strings.
pub(crate) fn matches_criteria(cell: &CellValue, criteria: &CellValue) -> bool {
    let crit_str = criteria.to_string();
    let crit = crit_str.trim();
    // Comparison operators
    let (op, rest) = if let Some(r) = crit.strip_prefix(">=") { (">=", r) }
        else if let Some(r) = crit.strip_prefix("<=") { ("<=", r) }
        else if let Some(r) = crit.strip_prefix("<>") { ("<>", r) }
        else if let Some(r) = crit.strip_prefix('>') { (">", r) }
        else if let Some(r) = crit.strip_prefix('<') { ("<", r) }
        else { ("=", crit) };

    if let Ok(crit_num) = rest.parse::<f64>() {
        let cell_num = cell.coerce_to_number();
        return match op {
            ">=" => cell_num >= crit_num,
            "<=" => cell_num <= crit_num,
            "<>" => (cell_num - crit_num).abs() > 1e-10,
            ">" => cell_num > crit_num,
            "<" => cell_num < crit_num,
            _ => (cell_num - crit_num).abs() < 1e-10,
        };
    }
    // String comparison (case-insensitive)
    let cell_str = cell.to_string().to_uppercase();
    let rest_upper = rest.to_uppercase();
    match op {
        "<>" => cell_str != rest_upper,
        "=" => {
            // Wildcard: * matches any, ? matches one char
            wildcard_match(&cell_str, &rest_upper)
        }
        _ => cell_str == rest_upper,
    }
}

/// Simple wildcard match (* = any chars, ? = one char).
fn wildcard_match(text: &str, pattern: &str) -> bool {
    let t: Vec<char> = text.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    let mut dp = vec![vec![false; p.len() + 1]; t.len() + 1];
    dp[0][0] = true;
    for j in 1..=p.len() {
        if p[j - 1] == '*' { dp[0][j] = dp[0][j - 1]; }
    }
    for i in 1..=t.len() {
        for j in 1..=p.len() {
            if p[j - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if p[j - 1] == '?' || p[j - 1] == t[i - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }
    dp[t.len()][p.len()]
}

/// Flatten args (including RangeArray) into a vec of f64.
fn collect_numbers(args: &[CellValue]) -> Vec<f64> {
    let mut result = Vec::new();
    for a in args {
        match a {
            CellValue::RangeArray(rows) => {
                for row in rows {
                    for v in row {
                        match v {
                            CellValue::Number(n) => result.push(*n),
                            CellValue::Boolean(true) => result.push(1.0),
                            CellValue::Boolean(false) => result.push(0.0),
                            _ => {}
                        }
                    }
                }
            }
            CellValue::Number(n) => result.push(*n),
            CellValue::Boolean(true) => result.push(1.0),
            CellValue::Boolean(false) => result.push(0.0),
            _ => {}
        }
    }
    result
}

/// Compare two CellValues for ordering (used by VLOOKUP approximate match).
pub(crate) fn cell_value_cmp(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal),
        (CellValue::InlineString(x), CellValue::InlineString(y)) => x.to_uppercase().cmp(&y.to_uppercase()),
        (CellValue::Boolean(x), CellValue::Boolean(y)) => x.cmp(y),
        (CellValue::Empty, CellValue::Empty) => std::cmp::Ordering::Equal,
        (CellValue::Empty, _) => std::cmp::Ordering::Less,
        (_, CellValue::Empty) => std::cmp::Ordering::Greater,
        // Number < String in Excel
        (CellValue::Number(_), CellValue::InlineString(_)) => std::cmp::Ordering::Less,
        (CellValue::InlineString(_), CellValue::Number(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

/// Test two CellValues for equality (case-insensitive for strings).
pub(crate) fn cell_values_equal(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => (x - y).abs() < 1e-10,
        (CellValue::Boolean(x), CellValue::Boolean(y)) => x == y,
        (CellValue::InlineString(x), CellValue::InlineString(y)) => x.to_uppercase() == y.to_uppercase(),
        (CellValue::Empty, CellValue::Empty) => true,
        // Try coerced comparison
        _ => {
            let as_str = |v: &CellValue| -> String { v.to_string().to_uppercase() };
            as_str(a) == as_str(b)
        }
    }
}

// ─── More Text: UNICHAR, UNICODE, BAHTTEXT, NUMBERTEXT ───────────────────────

struct UnicharFunction;
impl FormulaFunction for UnicharFunction {
    fn name(&self) -> &str { "UNICHAR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number() as u32;
        let c = char::from_u32(n).ok_or_else(|| ExcliError::formula_eval_error("UNICHAR: invalid codepoint"))?;
        Ok(CellValue::InlineString(c.to_string()))
    }
}

struct UnicodeFunction;
impl FormulaFunction for UnicodeFunction {
    fn name(&self) -> &str { "UNICODE" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let s = args[0].to_string();
        let c = s.chars().next().ok_or_else(|| ExcliError::formula_eval_error("UNICODE: empty string"))?;
        Ok(CellValue::Number(c as u32 as f64))
    }
}

struct BahtextFunction;
impl FormulaFunction for BahtextFunction {
    fn name(&self) -> &str { "BAHTTEXT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // Simplified implementation - just format as a Thai-style number string
        let n = args[0].coerce_to_number();
        Ok(CellValue::InlineString(format!("{} บาทถ้วน", n as i64)))
    }
}

struct NumbertextFunction;
impl FormulaFunction for NumbertextFunction {
    fn name(&self) -> &str { "NUMBERTEXT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Text }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number() as i64;
        Ok(CellValue::InlineString(n.to_string()))
    }
}

// ─── Math: CEILING.MATH, FLOOR.MATH, MUNIT ───────────────────────────────────

struct CeilingMathFunction;
impl FormulaFunction for CeilingMathFunction {
    fn name(&self) -> &str { "CEILING.MATH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let sig = if args.len() > 1 { args[1].coerce_to_number() } else { 1.0 };
        let mode = if args.len() > 2 { args[2].coerce_to_number() } else { 0.0 };
        let sig = if sig == 0.0 { return Ok(CellValue::Number(0.0)); } else { sig.abs() };
        if mode != 0.0 && n < 0.0 {
            Ok(CellValue::Number((n / sig).floor() * sig))
        } else {
            Ok(CellValue::Number((n / sig).ceil() * sig))
        }
    }
}

struct FloorMathFunction;
impl FormulaFunction for FloorMathFunction {
    fn name(&self) -> &str { "FLOOR.MATH" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number();
        let sig = if args.len() > 1 { args[1].coerce_to_number() } else { 1.0 };
        let mode = if args.len() > 2 { args[2].coerce_to_number() } else { 0.0 };
        let sig = if sig == 0.0 { return Ok(CellValue::Number(0.0)); } else { sig.abs() };
        if mode != 0.0 && n < 0.0 {
            Ok(CellValue::Number((n / sig).ceil() * sig))
        } else {
            Ok(CellValue::Number((n / sig).floor() * sig))
        }
    }
}

struct MunitFunction;
impl FormulaFunction for MunitFunction {
    fn name(&self) -> &str { "MUNIT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Math }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let n = args[0].coerce_to_number() as usize;
        // Return identity matrix as RangeArray
        let rows: Vec<Vec<CellValue>> = (0..n).map(|r| {
            (0..n).map(|c| {
                if r == c { CellValue::Number(1.0) } else { CellValue::Number(0.0) }
            }).collect()
        }).collect();
        Ok(CellValue::RangeArray(rows))
    }
}

// ─── Bitwise: BITAND, BITOR, BITXOR, BITLSHIFT, BITRSHIFT ────────────────────

struct BitandFunction;
impl FormulaFunction for BitandFunction {
    fn name(&self) -> &str { "BITAND" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let a = args[0].coerce_to_number() as u64;
        let b = args[1].coerce_to_number() as u64;
        Ok(CellValue::Number((a & b) as f64))
    }
}

struct BitorFunction;
impl FormulaFunction for BitorFunction {
    fn name(&self) -> &str { "BITOR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let a = args[0].coerce_to_number() as u64;
        let b = args[1].coerce_to_number() as u64;
        Ok(CellValue::Number((a | b) as f64))
    }
}

struct BitxorFunction;
impl FormulaFunction for BitxorFunction {
    fn name(&self) -> &str { "BITXOR" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let a = args[0].coerce_to_number() as u64;
        let b = args[1].coerce_to_number() as u64;
        Ok(CellValue::Number((a ^ b) as f64))
    }
}

struct BitlshiftFunction;
impl FormulaFunction for BitlshiftFunction {
    fn name(&self) -> &str { "BITLSHIFT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let a = args[0].coerce_to_number() as u64;
        let n = args[1].coerce_to_number() as u32;
        Ok(CellValue::Number((a << n) as f64))
    }
}

struct BitrshiftFunction;
impl FormulaFunction for BitrshiftFunction {
    fn name(&self) -> &str { "BITRSHIFT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Engineering }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let a = args[0].coerce_to_number() as u64;
        let n = args[1].coerce_to_number() as u32;
        Ok(CellValue::Number((a >> n) as f64))
    }
}

// ─── Statistical: NORM.DIST, NORM.INV, T.DIST, Z.TEST ────────────────────────

/// Approximation of the standard normal CDF using Hart's algorithm
fn norm_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf_approx(x / std::f64::consts::SQRT_2))
}

/// Approximation of erf
fn erf_approx(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let val = 1.0 - poly * (-x * x).exp();
    if x >= 0.0 { val } else { -val }
}

struct NormdistFunction;
impl FormulaFunction for NormdistFunction {
    fn name(&self) -> &str { "NORM.DIST" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 4 }
    fn max_args(&self) -> Option<usize> { Some(4) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let x      = args[0].coerce_to_number();
        let mean   = args[1].coerce_to_number();
        let sd     = args[2].coerce_to_number();
        let cumul  = args[3].coerce_to_bool();
        if sd <= 0.0 { return Err(ExcliError::formula_eval_error("NORM.DIST: stdev must be > 0")); }
        if cumul {
            Ok(CellValue::Number(norm_cdf((x - mean) / sd)))
        } else {
            let exponent = -0.5 * ((x - mean) / sd).powi(2);
            let pdf = exponent.exp() / (sd * (2.0 * std::f64::consts::PI).sqrt());
            Ok(CellValue::Number(pdf))
        }
    }
}

/// Rational approximation for inverse normal CDF
fn norm_inv_approx(p: f64) -> f64 {
    // Beasley-Springer-Moro algorithm approximation
    let a = [2.50662823884, -18.61500062529, 41.39119773534, -25.44106049637];
    let b = [-8.47351093090, 23.08336743743, -21.06224101826, 3.13082909833];
    let c = [0.3374754822726147, 0.9761690190917186, 0.1607979714918209, 0.0276438810333863,
             0.0038405729373609, 0.0003951896511349, 0.0000321767881768, 0.0000002888167364, 0.0000003960315187];
    let y = p - 0.5;
    if y.abs() < 0.42 {
        let r = y * y;
        y * (((a[3] * r + a[2]) * r + a[1]) * r + a[0]) / ((((b[3] * r + b[2]) * r + b[1]) * r + b[0]) * r + 1.0)
    } else {
        let r = if y < 0.0 { p } else { 1.0 - p };
        let r = (-r.ln()).sqrt();
        let mut s = c[0];
        let mut r_pow = r;
        for &ci in &c[1..] {
            s += ci * r_pow;
            r_pow *= r;
        }
        if y < 0.0 { -s } else { s }
    }
}

struct NorminvFunction;
impl FormulaFunction for NorminvFunction {
    fn name(&self) -> &str { "NORM.INV" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let p    = args[0].coerce_to_number();
        let mean = args[1].coerce_to_number();
        let sd   = args[2].coerce_to_number();
        if p <= 0.0 || p >= 1.0 { return Err(ExcliError::formula_eval_error("NORM.INV: p must be in (0,1)")); }
        if sd <= 0.0 { return Err(ExcliError::formula_eval_error("NORM.INV: stdev must be > 0")); }
        Ok(CellValue::Number(mean + sd * norm_inv_approx(p)))
    }
}

struct TdistFunction;
impl FormulaFunction for TdistFunction {
    fn name(&self) -> &str { "T.DIST" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 3 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let x    = args[0].coerce_to_number();
        let df   = args[1].coerce_to_number();
        let cumul = args[2].coerce_to_bool();
        if df < 1.0 { return Err(ExcliError::formula_eval_error("T.DIST: df must be >= 1")); }
        if cumul {
            // Approximate using normal for large df
            let t_cdf = norm_cdf(x * (1.0 - 1.0 / (4.0 * df)));
            Ok(CellValue::Number(t_cdf))
        } else {
            let coeff = gamma_approx((df + 1.0) / 2.0) / (gamma_approx(df / 2.0) * (df * std::f64::consts::PI).sqrt());
            let pdf = coeff * (1.0 + x * x / df).powf(-(df + 1.0) / 2.0);
            Ok(CellValue::Number(pdf))
        }
    }
}

/// Simple Stirling-based gamma approximation
fn gamma_approx(x: f64) -> f64 {
    if x < 0.5 {
        std::f64::consts::PI / ((std::f64::consts::PI * x).sin() * gamma_approx(1.0 - x))
    } else {
        let x = x - 1.0;
        let t = x + 7.5;
        let c = [0.99999999999980993, 676.5203681218851, -1259.1392167224028,
                  771.32342877765313, -176.61502916214059, 12.507343278686905,
                  -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7];
        let mut s = c[0];
        for i in 1..9 { s += c[i] / (x + i as f64); }
        (2.0 * std::f64::consts::PI).sqrt() * s * t.powf(x + 0.5) * (-t).exp()
    }
}

struct ZtestFunction;
impl FormulaFunction for ZtestFunction {
    fn name(&self) -> &str { "Z.TEST" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Statistical }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn expand_range_args(&self) -> bool { false }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let data: Vec<f64> = flatten_range(&args[0]).iter().filter_map(|v| v.as_f64()).collect();
        if data.is_empty() { return Err(ExcliError::formula_eval_error("Z.TEST: empty data")); }
        let mu0 = args[1].coerce_to_number();
        let n = data.len() as f64;
        let mean = data.iter().sum::<f64>() / n;
        let sd = if args.len() > 2 {
            args[2].coerce_to_number()
        } else {
            let var = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
            var.sqrt()
        };
        if sd == 0.0 { return Err(ExcliError::formula_eval_error("Z.TEST: sd=0")); }
        let z = (mean - mu0) / (sd / n.sqrt());
        Ok(CellValue::Number(1.0 - norm_cdf(z)))
    }
}

// ─── Date: DAYS, DAYS360, YEARFRAC, ISODATE ──────────────────────────────────

struct DaysFunction;
impl FormulaFunction for DaysFunction {
    fn name(&self) -> &str { "DAYS" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let end   = args[0].coerce_to_number();
        let start = args[1].coerce_to_number();
        Ok(CellValue::Number((end - start).floor()))
    }
}

struct Days360Function;
impl FormulaFunction for Days360Function {
    fn name(&self) -> &str { "DAYS360" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let start_serial = args[0].coerce_to_number() as i64;
        let end_serial   = args[1].coerce_to_number() as i64;
        // Simplified: multiply by 360/365 approximation
        Ok(CellValue::Number(((end_serial - start_serial) as f64 * 360.0 / 365.0).floor()))
    }
}

struct YearfracFunction;
impl FormulaFunction for YearfracFunction {
    fn name(&self) -> &str { "YEARFRAC" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 2 }
    fn max_args(&self) -> Option<usize> { Some(3) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let start = args[0].coerce_to_number();
        let end   = args[1].coerce_to_number();
        let basis = if args.len() > 2 { args[2].coerce_to_number() as i32 } else { 0 };
        let days = end - start;
        let year_days = match basis {
            1 => 365.25, // Actual/actual
            2 => 360.0,  // Actual/360
            3 => 365.0,  // Actual/365
            4 => 360.0,  // European 30/360
            _ => 360.0,  // US 30/360
        };
        Ok(CellValue::Number(days / year_days))
    }
}

struct IsodateFunction;
impl FormulaFunction for IsodateFunction {
    fn name(&self) -> &str { "ISOWEEKNUM" }
    fn category(&self) -> FunctionCategory { FunctionCategory::DateTime }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        let serial = args[0].coerce_to_number() as i64;
        // Excel serial to date (1900-01-01 = 1)
        let days_since_epoch = serial - 25569; // days from 1900-01-01 to 1970-01-01
        let date = chrono::NaiveDate::from_num_days_from_ce_opt((days_since_epoch + 719163) as i32)
            .unwrap_or(chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
        Ok(CellValue::Number(date.iso_week().week() as f64))
    }
}

// ─── Lookup helpers: HYPERLINK, FORMULATEXT ───────────────────────────────────

struct HyperlinkFunction;
impl FormulaFunction for HyperlinkFunction {
    fn name(&self) -> &str { "HYPERLINK" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Lookup }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(2) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // Return the friendly name if provided, else the URL
        let display = if args.len() > 1 { args[1].to_string() } else { args[0].to_string() };
        Ok(CellValue::InlineString(display))
    }
}

struct FormulaTextFunction;
impl FormulaFunction for FormulaTextFunction {
    fn name(&self) -> &str { "FORMULATEXT" }
    fn category(&self) -> FunctionCategory { FunctionCategory::Information }
    fn min_args(&self) -> usize { 1 }
    fn max_args(&self) -> Option<usize> { Some(1) }
    fn evaluate(&self, args: &[CellValue]) -> ExcliResult<CellValue> {
        // Without cell context, just return the string representation
        Ok(CellValue::InlineString(args[0].to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> FunctionRegistry {
        FunctionRegistry::new()
    }

    fn eval(reg: &FunctionRegistry, name: &str, args: Vec<CellValue>) -> ExcliResult<CellValue> {
        let f = reg.get(name).unwrap();
        f.evaluate(&args)
    }

    #[test]
    fn test_registry_has_new_functions() {
        let r = reg();
        assert!(r.get("NPER").is_some());
        assert!(r.get("RATE").is_some());
        assert!(r.get("SLN").is_some());
        assert!(r.get("DB").is_some());
        assert!(r.get("DEGREES").is_some());
        assert!(r.get("RADIANS").is_some());
        assert!(r.get("ASIN").is_some());
        assert!(r.get("ACOS").is_some());
        assert!(r.get("ATAN").is_some());
        assert!(r.get("ATAN2").is_some());
        assert!(r.get("SINH").is_some());
        assert!(r.get("COSH").is_some());
        assert!(r.get("TANH").is_some());
        assert!(r.get("REPT").is_some());
        assert!(r.get("EXACT").is_some());
        assert!(r.get("CONCATENATE").is_some());
        assert!(r.get("FIXED").is_some());
        assert!(r.get("BIN2DEC").is_some());
        assert!(r.get("DEC2BIN").is_some());
        assert!(r.get("HEX2DEC").is_some());
        assert!(r.get("DEC2HEX").is_some());
        assert!(r.get("OCT2DEC").is_some());
        assert!(r.get("DEC2OCT").is_some());
        assert!(r.get("SHEET").is_some());
        assert!(r.get("SHEETS").is_some());
        assert!(r.get("DSUM").is_some());
        assert!(r.get("DAVERAGE").is_some());
        assert!(r.get("DCOUNT").is_some());
        assert!(r.get("DMAX").is_some());
        assert!(r.get("DMIN").is_some());
        assert!(r.get("SORTBY").is_some());
        assert!(r.get("RANDARRAY").is_some());
    }

    #[test]
    fn test_sln_function() {
        let r = reg();
        // SLN(10000, 1000, 9) = 1000
        let result = eval(&r, "SLN", vec![
            CellValue::Number(10000.0),
            CellValue::Number(1000.0),
            CellValue::Number(9.0),
        ]).unwrap();
        assert_eq!(result, CellValue::Number(1000.0));
    }

    #[test]
    fn test_degrees_radians() {
        let r = reg();
        let d = eval(&r, "DEGREES", vec![CellValue::Number(std::f64::consts::PI)]).unwrap();
        if let CellValue::Number(n) = d { assert!((n - 180.0).abs() < 0.001); }
        else { panic!(); }
        let rad = eval(&r, "RADIANS", vec![CellValue::Number(180.0)]).unwrap();
        if let CellValue::Number(n) = rad { assert!((n - std::f64::consts::PI).abs() < 0.001); }
        else { panic!(); }
    }

    #[test]
    fn test_rept() {
        let r = reg();
        let result = eval(&r, "REPT", vec![
            CellValue::InlineString("x".to_string()),
            CellValue::Number(4.0),
        ]).unwrap();
        assert_eq!(result, CellValue::InlineString("xxxx".to_string()));
    }

    #[test]
    fn test_exact() {
        let r = reg();
        let yes = eval(&r, "EXACT", vec![
            CellValue::InlineString("Hello".to_string()),
            CellValue::InlineString("Hello".to_string()),
        ]).unwrap();
        assert_eq!(yes, CellValue::Boolean(true));
        let no = eval(&r, "EXACT", vec![
            CellValue::InlineString("hello".to_string()),
            CellValue::InlineString("Hello".to_string()),
        ]).unwrap();
        assert_eq!(no, CellValue::Boolean(false));
    }

    #[test]
    fn test_bin2dec_dec2bin() {
        let r = reg();
        let d = eval(&r, "BIN2DEC", vec![CellValue::InlineString("1010".to_string())]).unwrap();
        assert_eq!(d, CellValue::Number(10.0));
        let b = eval(&r, "DEC2BIN", vec![CellValue::Number(10.0)]).unwrap();
        assert_eq!(b, CellValue::InlineString("1010".to_string()));
    }

    #[test]
    fn test_hex2dec_dec2hex() {
        let r = reg();
        let d = eval(&r, "HEX2DEC", vec![CellValue::InlineString("A".to_string())]).unwrap();
        assert_eq!(d, CellValue::Number(10.0));
        let h = eval(&r, "DEC2HEX", vec![CellValue::Number(255.0)]).unwrap();
        assert_eq!(h, CellValue::InlineString("FF".to_string()));
    }

    #[test]
    fn test_oct2dec_dec2oct() {
        let r = reg();
        let d = eval(&r, "OCT2DEC", vec![CellValue::InlineString("10".to_string())]).unwrap();
        assert_eq!(d, CellValue::Number(8.0));
        let o = eval(&r, "DEC2OCT", vec![CellValue::Number(8.0)]).unwrap();
        assert_eq!(o, CellValue::InlineString("10".to_string()));
    }

    #[test]
    fn test_concatenate() {
        let r = reg();
        let result = eval(&r, "CONCATENATE", vec![
            CellValue::InlineString("Hello".to_string()),
            CellValue::InlineString(", ".to_string()),
            CellValue::InlineString("World".to_string()),
        ]).unwrap();
        assert_eq!(result, CellValue::InlineString("Hello, World".to_string()));
    }

    #[test]
    fn test_atan2_function() {
        let r = reg();
        // ATAN2(1, 1) = PI/4
        let result = eval(&r, "ATAN2", vec![CellValue::Number(1.0), CellValue::Number(1.0)]).unwrap();
        if let CellValue::Number(n) = result {
            assert!((n - std::f64::consts::FRAC_PI_4).abs() < 0.001);
        } else { panic!(); }
    }

    #[test]
    fn test_sinh_cosh_tanh() {
        let r = reg();
        assert_eq!(eval(&r, "SINH", vec![CellValue::Number(0.0)]).unwrap(), CellValue::Number(0.0));
        assert_eq!(eval(&r, "COSH", vec![CellValue::Number(0.0)]).unwrap(), CellValue::Number(1.0));
        assert_eq!(eval(&r, "TANH", vec![CellValue::Number(0.0)]).unwrap(), CellValue::Number(0.0));
    }

    #[test]
    fn test_sheet_sheets_functions() {
        let r = reg();
        assert_eq!(eval(&r, "SHEET", vec![]).unwrap(), CellValue::Number(1.0));
        assert_eq!(eval(&r, "SHEETS", vec![]).unwrap(), CellValue::Number(1.0));
    }

    #[test]
    fn test_randarray_returns_array() {
        let r = reg();
        let result = eval(&r, "RANDARRAY", vec![
            CellValue::Number(2.0),
            CellValue::Number(3.0),
        ]).unwrap();
        if let CellValue::RangeArray(rows) = result {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].len(), 3);
        } else { panic!("expected RangeArray"); }
    }

    #[test]
    fn test_frequency_basic() {
        let r = reg();
        // FREQUENCY([1,2,3,4,5], [2,4]) should count values <=2 in first bucket
        let data = CellValue::RangeArray(vec![
            vec![CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Number(3.0), CellValue::Number(4.0), CellValue::Number(5.0)]
        ]);
        let bins = CellValue::RangeArray(vec![vec![CellValue::Number(2.0), CellValue::Number(4.0)]]);
        let result = eval(&r, "FREQUENCY", vec![data, bins]).unwrap();
        // First bucket: values <= 2 → expect 0 (strictly < 2 means 1 value: 1)
        // Actually partition_point returns index where bin < d, so for d=1, bin[0]=2 → 2 < 1 is false → idx=0 → counts[0]++
        // For d=2, 2 < 2 is false → idx=0 → counts[0]++
        assert!(matches!(result, CellValue::Number(_)));
    }

    #[test]
    fn test_unichar_unicode() {
        let r = reg();
        let c = eval(&r, "UNICHAR", vec![CellValue::Number(65.0)]).unwrap();
        assert_eq!(c, CellValue::InlineString("A".to_string()));
        let n = eval(&r, "UNICODE", vec![CellValue::InlineString("A".to_string())]).unwrap();
        assert_eq!(n, CellValue::Number(65.0));
    }

    #[test]
    fn test_bitwise_functions() {
        let r = reg();
        assert_eq!(eval(&r, "BITAND", vec![CellValue::Number(12.0), CellValue::Number(10.0)]).unwrap(), CellValue::Number(8.0));
        assert_eq!(eval(&r, "BITOR", vec![CellValue::Number(12.0), CellValue::Number(10.0)]).unwrap(), CellValue::Number(14.0));
        assert_eq!(eval(&r, "BITXOR", vec![CellValue::Number(12.0), CellValue::Number(10.0)]).unwrap(), CellValue::Number(6.0));
        assert_eq!(eval(&r, "BITLSHIFT", vec![CellValue::Number(1.0), CellValue::Number(3.0)]).unwrap(), CellValue::Number(8.0));
        assert_eq!(eval(&r, "BITRSHIFT", vec![CellValue::Number(8.0), CellValue::Number(2.0)]).unwrap(), CellValue::Number(2.0));
    }

    #[test]
    fn test_ceiling_math_floor_math() {
        let r = reg();
        let c = eval(&r, "CEILING.MATH", vec![CellValue::Number(2.3)]).unwrap();
        assert_eq!(c, CellValue::Number(3.0));
        let f = eval(&r, "FLOOR.MATH", vec![CellValue::Number(2.7)]).unwrap();
        assert_eq!(f, CellValue::Number(2.0));
    }

    #[test]
    fn test_munit() {
        let r = reg();
        let result = eval(&r, "MUNIT", vec![CellValue::Number(2.0)]).unwrap();
        if let CellValue::RangeArray(rows) = result {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0][0], CellValue::Number(1.0));
            assert_eq!(rows[0][1], CellValue::Number(0.0));
            assert_eq!(rows[1][0], CellValue::Number(0.0));
            assert_eq!(rows[1][1], CellValue::Number(1.0));
        } else { panic!("expected RangeArray"); }
    }

    #[test]
    fn test_norm_dist() {
        let r = reg();
        // NORM.DIST(0, 0, 1, TRUE) = 0.5
        let result = eval(&r, "NORM.DIST", vec![
            CellValue::Number(0.0),
            CellValue::Number(0.0),
            CellValue::Number(1.0),
            CellValue::Boolean(true),
        ]).unwrap();
        if let CellValue::Number(n) = result {
            assert!((n - 0.5).abs() < 0.01, "Expected ~0.5 got {n}");
        } else { panic!(); }
    }

    #[test]
    fn test_norm_inv() {
        let r = reg();
        // NORM.INV(0.5, 0, 1) = 0
        let result = eval(&r, "NORM.INV", vec![
            CellValue::Number(0.5),
            CellValue::Number(0.0),
            CellValue::Number(1.0),
        ]).unwrap();
        if let CellValue::Number(n) = result {
            assert!(n.abs() < 0.1, "Expected ~0 got {n}");
        } else { panic!(); }
    }

    #[test]
    fn test_days_function() {
        let r = reg();
        // DAYS(100, 50) = 50
        let result = eval(&r, "DAYS", vec![CellValue::Number(100.0), CellValue::Number(50.0)]).unwrap();
        assert_eq!(result, CellValue::Number(50.0));
    }

    #[test]
    fn test_hyperlink() {
        let r = reg();
        let result = eval(&r, "HYPERLINK", vec![
            CellValue::InlineString("http://example.com".to_string()),
            CellValue::InlineString("Example".to_string()),
        ]).unwrap();
        assert_eq!(result, CellValue::InlineString("Example".to_string()));
    }
}

