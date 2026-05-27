# Doc-tests 在哪里？

Doc-tests 是在 **代码注释中** 的可执行代码示例。

## 📍 Doc-tests 的位置

### 位置 1: 公开函数的注释

```rust
// 文件: excli-core/src/cell.rs

/// Get the numeric value of a cell.
/// 
/// # Examples
/// ```
/// use excli_core::cell::CellValue;
/// let num = CellValue::Number(42.0);
/// assert_eq!(num.coerce_to_number(), 42.0);
/// ```
pub fn coerce_to_number(&self) -> f64 {
    // ...
}
```

### 位置 2: 公开结构体的注释

```rust
// 文件: excli-core/src/cell.rs

/// A cell in a spreadsheet.
///
/// # Examples
/// ```
/// use excli_core::cell::Cell;
/// let cell = Cell::number(10.5);
/// assert!(cell.value.is_number());
/// ```
pub struct Cell {
    pub value: CellValue,
    // ...
}
```

### 位置 3: 公开枚举的注释

```rust
// 文件: excli-core/src/cell.rs

/// Values that can be stored in a cell.
///
/// # Examples
/// ```
/// use excli_core::cell::CellValue;
/// assert!(matches!(CellValue::Empty, CellValue::Empty));
/// ```
pub enum CellValue {
    // ...
}
```

### 位置 4: 模块级文档（crate root）

```rust
// 文件: excli-core/src/lib.rs

//! excli-core: Core data structures for spreadsheets
//!
//! # Examples
//! ```
//! use excli_core::Workbook;
//! let wb = Workbook::new();
//! assert_eq!(wb.sheet_count(), 0);
//! ```

pub mod cell;
pub mod sheet;
// ...
```

---

## 🔍 当前代码中的 Doc 注释位置

所有这些文件都有 `///` 注释（文档），但还没有 `# Examples` 代码块：

| 文件 | 注释数 | Doc-tests |
|------|-------|----------|
| excli-core/src/cell.rs | ✅ 有 | ❌ 无 |
| excli-core/src/sheet.rs | ✅ 有 | ❌ 无 |
| excli-formula/src/evaluator.rs | ✅ 有 | ❌ 无 |
| excli-ops/src/sort.rs | ✅ 有 | ❌ 无 |
| excli-io/src/reader.rs | ✅ 有 | ❌ 无 |

---

## 📝 添加 Doc-test 的步骤

### 步骤 1: 找到要添加的函数

打开任意源文件，例如 `excli-core/src/cell.rs`

### 步骤 2: 在注释中添加 `# Examples` 块

```rust
/// Converts the cell value to a number.
///
/// # Examples
/// ```
/// use excli_core::cell::CellValue;
/// 
/// let num = CellValue::Number(42.0);
/// assert_eq!(num.coerce_to_number(), 42.0);
/// ```
pub fn coerce_to_number(&self) -> f64 {
    match self {
        CellValue::Number(n) => *n,
        CellValue::Boolean(true) => 1.0,
        CellValue::Boolean(false) => 0.0,
        CellValue::Empty => 0.0,
        _ => f64::NAN,
    }
}
```

### 步骤 3: 运行 doc-tests

```bash
cargo test --doc
```

---

## 🎯 常见的 Doc-test 模板

### 简单函数示例
```rust
/// Returns the absolute value.
///
/// # Examples
/// ```
/// assert_eq!(5, 5);
/// assert_eq!(0, 0);
/// ```
pub fn abs_value(x: i32) -> i32 {
    x.abs()
}
```

### 带导入的示例
```rust
/// Creates a new cell with a number value.
///
/// # Examples
/// ```
/// use excli_core::cell::Cell;
/// let cell = Cell::number(42.0);
/// assert!(cell.value.is_number());
/// ```
pub fn number(value: f64) -> Cell {
    Cell { value: CellValue::Number(value) }
}
```

### 会失败/panic 的示例
```rust
/// # Panics
/// Panics if the index is out of bounds.
///
/// # Examples
/// ```should_panic
/// let arr = [1, 2, 3];
/// let _ = arr[10];  // This panics
/// ```
pub fn get(arr: &[i32], idx: usize) -> i32 {
    arr[idx]
}
```

---

## 💡 为什么当前都是 0 个？

因为文件中只有文档注释 `///`，但**没有**包含 `# Examples` 代码块。

doc-tests 的触发条件：
- ✅ 必须在 `///` 注释中
- ✅ 必须有 ` # Examples` 或其他代码块
- ✅ ` ```rust` 或 ` ```code fence
- ❌ 空的注释 → 0 个 doc-test

---

## 🚀 快速命令

```bash
# 运行所有 doc-tests
cargo test --doc

# 运行特定 crate 的 doc-tests
cargo test --doc -p excli-core

# 显示 doc-tests 的输出
cargo test --doc -- --nocapture

# 生成文档并在浏览器打开
cargo doc --open
```
