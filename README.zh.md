# excli — Rust 电子表格处理命令行工具

`excli` 是一个基于 Rust 的命令行工具与多 crate 工作区，用于读取、转换、校验和写入电子表格数据，适合批处理任务、Shell 管道以及结构化数据集成场景。

[English](README.md) | **简体中文**

## 能力概览

### 核心特征

- **格式互操作** — 可读取 `.xlsx`、`.xlsm`、`.xlsb`、`.xls`、`.ods`、`.csv`、`.tsv`；可写入新的 `.xlsx`、`.csv`、`.tsv`、`.json` 文件
- **面向管道的 I/O** — 提供终端可读输出和 JSON 结构化输出，便于自动化处理
- **编码与 Unicode 处理** — 包含编码检测、排序规则和规范化相关能力
- **公式处理** — 支持公式解析、求值和依赖分析
- **单二进制部署** — 无需 Python、Java 或 Node.js 运行时
- **可组合命令设计** — 子命令适合在 Unix 风格数据管道中串联使用

### 功能域

| 功能 | 状态 | 说明 |
|------|------|------|
| **读取** | ⚠️ 部分 | 单元格值、公式及部分元数据；尚未完整建模高级 Excel 特性 |
| **写入** | ⚠️ 有限 | 新建 XLSX 和单工作表 CSV/TSV；修改现有 Excel 须使用保留原文件结构的操作 |
| **过滤** | ✅ 可用 | 条件过滤、列选择、正则表达式 |
| **排序** | ✅ 可用 | 多关键字排序与稳定排序 |
| **聚合** | ✅ 可用 | 汇总统计、分组聚合、透视类操作 |
| **转换** | ✅ 可用 | 投影、转置、重塑和列派生 |
| **合并** | ✅ 可用 | 横向/纵向合并与去重处理 |
| **搜索** | ✅ 可用 | 全文检索、正则匹配、模糊匹配 |
| **公式** | ⚠️ 部分 | 解析、求值、依赖检查及 XLSX/XLSM 普通公式编辑 |
| **样式** | ⚠️ 有限 | 内存中的样式操作；尚不支持完整的 Excel 样式往返保真 |

---

## 修改现有 Excel 工作簿

读取 XLSX/XLSM 时可保留公式文本与缓存值、富文本分段、常见单元格/行/列样式、自定义数字格式及原始格式 ID、日期/时间的数值序列、命名区域的工作簿或工作表作用域、隐藏状态和备注、外部工作簿链接路径及其工作表名称、常见数据连接类型与来源信息、工作簿日期系统、常见工作表数据验证规则（含 Office 2010 扩展规则）、工作表自动筛选范围及条件、条件格式的范围与优先级（解析简单色阶、数据条，以及使用受支持差异样式的公式和单值比较规则；其余规则保留原始 XML，Office 2010 扩展组单独暴露其原始 XML）、表格名称/范围/列、常见透视表的名称、位置、工作表来源与字段布局、传统单元格批注的文字与作者、现代线程批注的回复关系、作者姓名及提及位置、嵌入图片的字节内容与位置、常见图表元数据、工作表保护权限、打印区域与重复标题行列，以及部分工作表布局信息。值列表、Top N 个数和动态筛选条件会解析为模型类型；复合条件的原始 XML 保存在 `FilterType::Custom` 中。内存模型尚未完整表示所有高级 Excel 特性。

`excli replace report.xlsx --find Old --replace New` 可以替换现有 `.xlsx` 或 `.xlsm` 中的普通文本单元格，同时保留其他文件内容，包括公式、格式和宏；替换带命名空间前缀的工作表单元格时也会保留该前缀。富文本的查找内容若完全落在单个分段内，可保留该分段格式完成替换；常见共享富文本单元格会转换为行内富文本。跨分段匹配或依赖额外命名空间的内容仍会报错，原文件保持不变。可以先加 `--dry-run` 查看命中的单元格数量。

`excli sheet hide report.xlsx --sheet Archive` 和 `excli sheet unhide report.xlsx --sheet Archive` 可在不重建工作簿的情况下修改工作表显隐状态。`excli formula set report.xlsx --sheet Sales --cell B2 --formula '=A1+1'` 可写入普通公式并保留其他文件内容，同时请求 Excel 打开时重新计算。公式及带类型的值编辑也会保留工作表元素前缀。共享公式、数组公式、富文本单元格和带扩展元数据的单元格暂不支持公式修改。数字签名工作簿不可编辑。

`excli sheet move report.xlsx --sheet Archive --index 0` 可重排工作表标签，同时保留工作表文件、公式和包关系；局部命名区域及工作簿视图中的工作表索引会同步调整。

`excli sheet add report.xlsx --name Archive` 可在现有 XLSX/XLSM 文件中新增空白工作表，并保留原工作表文件。无效或重复的 Excel 工作表名称会被拒绝。

`excli sheet copy report.xlsx --source Sales --dest SalesCopy` 可在现有 XLSX/XLSM 包内复制工作表 XML，保留单元格、公式、格式和超链接；副本中的显式源表引用及复制出的工作表局部命名区域会改为新表名。附带表格会获得新的包部件、名称和 ID；传统批注与现代批注线程会获得独立部件，线程批注和提及 ID 也会更新。工作表绘图和常见图表会复制为独立部件，图表中的源表引用会更新，嵌入图片继续引用原有媒体数据。常见透视表会获得独立定义和唯一名称，继续引用原有共享源缓存。暂不支持的绘图或透视表关系、带下级包关系的图表、需要改写表格名称的公式，以及含 VBA 项目或计算链的工作簿，仍会明确报错。

`excli sheet rename report.xlsx Data Sales` 可在不重建工作簿的情况下修改工作表名称，并同步修改常见的工作表、定义名称、图表、表格和数据验证公式引用；公式中的字符串常量保持不变。若仍有无法处理的引用，命令会报错并保持原文件不变。

`excli sheet delete report.xlsx --sheet Archive` 可删除工作表及其独占的包部件，同时保留其余工作表。若其他部件仍引用该工作表、删除后没有可见工作表，或文件含有 VBA 项目或计算链，命令会报错并保持原文件不变。

`excli write report.xlsx --append --sheet Sales --data '[[12.5,true,"新项目"]]'` 可向现有 XLSX/XLSM 工作表追加带类型的值，并保留文件中的其他部分。对现有 Excel 文件执行不带 `--append` 的 `write` 会报错，避免覆盖整个工作簿。富文本单元格、公式组和合并区域中非左上角的单元格暂不支持这种编辑。

传入 CSV 文本时可正确解析引号包裹的逗号和换行，例如 `excli write report.xlsx --append --sheet Sales --data '"项目, 一",12.5'`。

其他需要重建现有 Excel 工作簿的命令目前会明确报错，避免静默丢失公式、格式或工作簿特性。不支持重建 `.xlsm`、`.xls`、`.ods` 等格式。导出 CSV/TSV 时只能包含一个工作表，例如 `excli convert report.xlsx report.csv --sheet Sales`。

若输入输出格式相同且未使用 `--sheet`，`excli convert report.xlsx backup.xlsx` 会逐字节复制原文件；`.xlsm`、`.xlsb`、`.xls` 和 `.ods` 也支持这种原样复制。对于 XLSX，`excli convert report.xlsx selected.xlsx --sheet Sales` 可通过包级编辑提取单张工作表，保留其 XML 和关联部件。如果保留的部件仍引用被删除的工作表，提取会报错且不改变输出文件。

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
excli convert sales.xlsx output.csv --sheet Sales

# CSV 转 Excel
excli convert data.csv result.xlsx

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
excli formula set <file> --sheet NAME --cell B2 --formula EXPR
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
