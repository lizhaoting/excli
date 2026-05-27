# excli — Rust 电子表格处理命令行工具

`excli` 是一个基于 Rust 的命令行工具与多 crate 工作区，用于读取、转换、校验和写入电子表格数据，适合批处理任务、Shell 管道以及结构化数据集成场景。

[English](README.md) | **简体中文**

## 能力概览

### 核心特征

- **格式互操作** — 支持 `.xlsx`、`.xls`、`.xlsm`、`.ods`、`.csv` 等常见表格格式的读写
- **面向管道的 I/O** — 提供终端可读输出和 JSON 结构化输出，便于自动化处理
- **编码与 Unicode 处理** — 包含编码检测、排序规则和规范化相关能力
- **公式处理** — 支持公式解析、求值和依赖分析
- **单二进制部署** — 无需 Python、Java 或 Node.js 运行时
- **可组合命令设计** — 子命令适合在 Unix 风格数据管道中串联使用

### 功能域

| 功能 | 状态 | 说明 |
|------|------|------|
| **读取** | ✅ 可用 | 多工作表访问、区域选择、格式检测 |
| **写入** | ✅ 可用 | 追加写入、格式转换、样式保留 |
| **过滤** | ✅ 可用 | 条件过滤、列选择、正则表达式 |
| **排序** | ✅ 可用 | 多关键字排序与稳定排序 |
| **聚合** | ✅ 可用 | 汇总统计、分组聚合、透视类操作 |
| **转换** | ✅ 可用 | 投影、转置、重塑和列派生 |
| **合并** | ✅ 可用 | 横向/纵向合并与去重处理 |
| **搜索** | ✅ 可用 | 全文检索、正则匹配、模糊匹配 |
| **公式** | ✅ 可用 | 公式解析、求值与依赖检查 |
| **样式** | ✅ 可用 | 主题、字体、颜色与条件格式 |

---

## 快速开始

### 安装

```bash
cargo install excli
```

### 基础用法

```bash
# 显示电子表格
excli read data.xlsx

# 只显示前 10 行
excli read data.xlsx --nrows 10

# JSON 输出（用于自动化处理）
excli read data.xlsx --json

# 统计信息
excli read data.xlsx --stats

# 指定 Sheet 和范围
excli read data.xlsx --sheet Sales --range A1:D100
```

### 数据处理

```bash
# 按条件过滤行
excli filter data.xlsx --where "age > 30 AND status = 'active'"

# 多字段排序
excli sort data.xlsx --by Name ASC --then Age DESC

# 分组聚合
excli aggregate data.xlsx --group-by Department --count --sum Salary

# 透视表
excli pivot data.xlsx --rows Region --columns Category --values Sales
```

### 格式转换

```bash
# Excel 转 CSV
excli read sales.xlsx | excli write output.csv

# CSV 转 Excel（带表头）
excli read data.csv --header | excli write result.xlsx

# Excel 转 JSON
excli read data.xlsx --json > data.json
```

---

## 架构设计

### 工作区布局

```
excli/
├── excli-cli          # 命令行界面
├── excli-core         # 核心数据结构（Cell、Sheet、Workbook）
├── excli-formula      # 公式解析/求值（200+ 函数）
├── excli-i18n         # 国际化（编码、排序、规范化）
├── excli-io           # 格式 I/O（Excel、ODS、CSV 等）
├── excli-ops          # 数据操作（排序、过滤、聚合、合并）
└── excli-style        # 样式管理（颜色、主题、条件格式）
```

### 设计原则

1. **职责分离** — CLI、I/O、核心数据结构、公式、国际化和样式能力按 crate 划分。
2. **管道组合** — 子命令适合与 Shell 管道组合处理表格数据。
3. **结构化输出** — `--json` 等输出模式用于程序间集成。
4. **类型化错误处理** — 通过 Rust 错误类型和 CLI 诊断暴露失败原因。
5. **面向批处理** — 实现侧重非交互式任务中的可预测执行行为。

---

## 命令参考

当前 CLI 暴露的一级操作如下：

```bash
excli read <file>                 # 读取工作表数据
excli write <output>              # 写入工作表数据
excli sheet <subcommand>          # 工作表管理操作
excli formula <subcommand>        # 公式读取与编辑
excli calc                        # 求值表达式或工作簿公式
excli filter <file>               # 过滤行
excli sort <file>                 # 排序行
excli agg <file>                  # 聚合或透视数据
excli merge <file1> <file2>       # 连接两个数据集
excli concat <files...>           # 拼接多个工作簿
excli transpose <file>            # 转置工作表
excli split <file>                # 按行数或列值拆分文件
excli sample <file>               # 抽样行
excli search <file>               # 搜索工作簿内容
excli replace <file>              # 替换工作簿内容
excli info <file>                 # 查看文件元数据
excli convert <input> <output>    # 在支持格式间转换
excli clean <file>                # 执行数据清洗操作
excli diff <file1> <file2>        # 比较两个文件
excli batch <glob> <command...>   # 对多个文件批量执行命令
excli detect-encoding <file>      # 检测文件编码
excli detect-lang <file>          # 检测内容语言
excli validate <file>             # 与参考文件进行比对
excli describe [command]          # 输出命令结构元数据
```

### `sheet` 子命令

```bash
excli sheet list <file>                # 列出工作表
excli sheet info <file> --sheet NAME   # 查看工作表元数据
excli sheet rename <file> OLD NEW      # 重命名工作表
excli sheet add <file> --name NAME     # 新增工作表
excli sheet delete <file> --sheet NAME # 删除工作表
excli sheet copy <file> --source A --dest B
excli sheet move <file> --sheet NAME --index N
excli sheet hide <file> --sheet NAME
excli sheet unhide <file> --sheet NAME
```

### `formula` 子命令

```bash
excli formula read <file>              # 读取公式
excli formula set <file> --formula EXPR
excli formula deps <file> --cell A1    # 查看依赖项
excli formula dependents <file> --cell A1
excli formula errors <file>            # 列出公式错误
excli formula circular <file>          # 检测循环引用
```

---

## 测试

仓库包含单元测试、集成测试和文档测试。可使用以下命令执行本地验证：

```bash
cargo test           # 运行所有测试
cargo test --doc     # 运行文档测试
make test-clean      # 使用项目测试快捷命令
```

---

## 贡献

欢迎提交改进。开发流程与约定见 [CONTRIBUTING.md](CONTRIBUTING.md)。

### 开发环境

```bash
git clone https://github.com/excli/excli.git
cd excli
cargo build
cargo test
```

---

## 许可证

Apache License 2.0 — 详见 [LICENSE](LICENSE)

---

## 相关资源

- [文档](https://docs.rs/excli)
- [问题反馈](https://github.com/excli/excli/issues)
- [讨论](https://github.com/excli/excli/discussions)

---

由 excli 贡献者维护。
