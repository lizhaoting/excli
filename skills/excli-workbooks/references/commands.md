# excli command examples

These examples match the current CLI help in the excli source repository. The binary may be named `excli` when installed or `excli-cli` when built from this workspace. Replace `excli` with the executable actually found.

## Inspect

```sh
excli info report.xlsx
excli sheet list report.xlsx
excli read report.xlsx --sheet Sales --range A1:D20 --json
```

## Preserve an existing XLSX/XLSM package

The following commands edit the named file in place. To deliver a separate output, copy the workbook to the requested output path first, then run the edit on that copy.

```sh
excli replace report.xlsx --find Old --replace New --dry-run
excli replace report.xlsx --find Old --replace New
excli write report.xlsx --append --sheet Sales --data '[[12.5,true,"new item"]]'
excli formula set report.xlsx --sheet Sales --cell B2 --formula '=A1+1'
excli sheet rename report.xlsx Data Sales
excli sheet add report.xlsx --name Archive
excli sheet copy report.xlsx --source Sales --dest SalesCopy
excli sheet move report.xlsx --sheet Archive --index 0
excli sheet hide report.xlsx --sheet Archive
excli sheet unhide report.xlsx --sheet Archive
excli sheet delete report.xlsx --sheet Archive
```

Use `replace --dry-run` before a text replacement when the user has not already supplied an exact expected count. The CLI rejects edits it cannot preserve safely; surface that error rather than masking it.

## Convert or extract

```sh
excli convert report.xlsx backup.xlsx
excli convert report.xlsx selected.xlsx --sheet Sales
excli convert report.xlsx sales.csv --sheet Sales
excli convert sales.csv new.xlsx
```

The first command copies an unchanged workbook byte for byte. The second extracts one sheet from XLSX through package edits. CSV export writes one sheet and loses workbook-only features by design. Check `excli convert --help` for delimiter and encoding options.
