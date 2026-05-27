# Test Output Optimization Guide

现在有多种方式快速查看测试结果，比直接运行 `cargo test` 清爽得多！

## 了解 0 个单元测试是什么？

你看到的 0 个单元测试来自两部分：

### 1. **main.rs (Binary Target)** 
- `excli-cli/src/main.rs` - CLI 程序本身，通常没有单元测试（正常）

### 2. **Doc-tests** 
- 代码注释中的 `/// code example` 块
- 所有 crate 的 doc-tests 都是 0，因为还没有添加文档示例
- 这是 **正常现象** - 不影响项目质量

---

## 方法 1️⃣ : 最干净的脚本（推荐）

```bash
./test-clean.sh
```

**输出示例：**
```
🧪 excli unit tests

📊 Unit test results (with tests):
test result: ok. 112 passed; 0 failed; ...
test result: ok. 18 passed; 0 failed; ...
test result: ok. 96 passed; 0 failed; ...
test result: ok. 239 passed; 0 failed; ...
...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ All unit tests passed: 672 total

💡 What are those 0s?
   • main.rs (binary) has no unit tests - normal
   • Doc-tests = /// code examples in comments - all empty (normal)
```

**优点：** 自动隐藏 0 个的结果，只显示有实际测试的，最清爽！

---

## 方法 2️⃣ : 标准脚本（显示所有）

```bash
./test.sh
```

显示所有 crate 的测试结果，包括 0 个的（便于检查遗漏）

---

## 方法 3️⃣ : 使用 Makefile

### 快速查看结果
```bash
make test
```
显示所有 crate 的测试结果

### 针对特定 crate
```bash
make test-core      # excli-core tests
make test-formula   # excli-formula tests
make test-io        # excli-io tests
make test-cli       # excli-cli tests
make test-ops       # excli-ops tests
make test-style     # excli-style tests
make test-i18n      # excli-i18n tests
```

### 其他有用命令
```bash
make build           # Debug 编译
make build-release   # Release 编译
make check           # 快速检查（不编译）
make fmt             # 代码格式化
make lint            # 运行 clippy 检查
make clean           # 清理构建产物
make help            # 显示所有命令
```

---

## 对比

| 方法 | 命令 | 输出 | 推荐 |
|------|------|------|------|
| 最干净 | `./test-clean.sh` | 仅显示有测试的部分 | ⭐⭐⭐ |
| 标准 | `./test.sh` | 完整逐行结果 | ⭐⭐ |
| Makefile | `make test` | 逐行显示 | ⭐⭐ |
| 原生 | `cargo test --all` | 完整日志（冗长） | - |

---

## 推荐工作流

1. **快速验证** → `./test-clean.sh` (2秒，最清爽)
2. **调试失败** → `cargo test -p <crate> -- --nocapture`
3. **完整检查** → `make fmt && make lint && cargo build --release`

---

## 如果想添加 Doc-tests

可以在代码注释中添加例子（可选，项目已经有很好的单元测试了）：

```rust
/// Get cell value
///
/// # Examples
/// ```
/// use excli_core::cell::Cell;
/// let cell = Cell::number(42.0);
/// assert_eq!(cell.value.coerce_to_number(), 42.0);
/// ```
pub fn get_value(&self) -> CellValue {
    // ...
}
```
