use clap::{Parser, Subcommand};

/// excli — High-performance CLI Excel processing tool
#[derive(Parser, Debug)]
#[command(name = "excli", version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output mode: human-readable or JSON
    #[arg(long, global = true)]
    pub json: bool,

    /// Quiet mode — data only
    #[arg(long, short, global = true)]
    pub quiet: bool,

    /// No emoji output
    #[arg(long, global = true)]
    pub no_emoji: bool,

    /// Verbose mode (-v, -vv, -vvv)
    #[arg(long, short, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Disable progress bars
    #[arg(long, global = true)]
    pub no_progress: bool,

    /// Output command JSON Schema (for AI Agent)
    #[arg(long, global = true)]
    pub describe: bool,

    /// Timeout in seconds
    #[arg(long, global = true, default_value = "30")]
    pub timeout: u32,

    /// Maximum memory in MB
    #[arg(long, global = true, default_value = "1024")]
    pub max_memory: u32,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Read data from an Excel file
    Read {
        file: String,

        #[arg(long)]
        sheet: Option<String>,

        #[arg(long)]
        range: Option<String>,

        #[arg(long, default_value = "0")]
        skip_rows: usize,

        #[arg(long)]
        nrows: Option<usize>,

        #[arg(long)]
        header: bool,

        #[arg(long)]
        no_index: bool,

        #[arg(long)]
        json_schema: bool,

        #[arg(long)]
        stats: bool,

        #[arg(long)]
        sample: Option<usize>,

        #[arg(long)]
        encoding: Option<String>,
    },

    /// Write data to an Excel file
    Write {
        output: String,

        #[arg(long)]
        data: Option<String>,

        #[arg(long)]
        sheet: Option<String>,

        #[arg(long)]
        start_cell: Option<String>,

        #[arg(long)]
        append: bool,

        #[arg(long)]
        header: Option<String>,

        #[arg(long)]
        no_header: bool,

        #[arg(long)]
        format: Option<String>,
    },

    /// Sheet operations
    Sheet {
        #[command(subcommand)]
        action: SheetCmd,
    },

    /// Formula operations
    Formula {
        #[command(subcommand)]
        action: FormulaCmd,
    },

    /// Calculation engine
    Calc {
        #[arg(long)]
        file: Option<String>,

        #[arg(long)]
        sheet: Option<String>,

        #[arg(long)]
        range: Option<String>,

        #[arg(long)]
        no_cache: bool,

        #[arg(long)]
        iterations: Option<u32>,

        #[arg(long)]
        evaluate: Option<String>,

        #[arg(long)]
        context: Option<String>,
    },

    /// Filter operations
    Filter {
        file: String,

        #[arg(long)]
        column: Option<String>,

        #[arg(long)]
        gt: Option<String>,

        #[arg(long)]
        lt: Option<String>,

        #[arg(long)]
        contains: Option<String>,

        #[arg(long)]
        between: Option<String>,

        #[arg(long)]
        r#in: Option<String>,

        #[arg(long)]
        is_null: bool,

        #[arg(long)]
        regex: Option<String>,

        #[arg(long)]
        expr: Option<String>,

        #[arg(long)]
        unique: bool,
    },

    /// Sort operations
    Sort {
        file: String,

        #[arg(long)]
        column: Option<String>,

        #[arg(long)]
        columns: Option<String>,

        #[arg(long)]
        asc: bool,

        #[arg(long)]
        desc: bool,

        #[arg(long)]
        custom: Option<String>,
    },

    /// Aggregate operations
    Agg {
        file: String,

        #[arg(long)]
        group_by: Option<String>,

        #[arg(long)]
        sum: Option<String>,

        #[arg(long)]
        count: bool,

        #[arg(long)]
        avg: Option<String>,

        #[arg(long)]
        pivot: bool,

        #[arg(long)]
        rows: Option<String>,

        #[arg(long)]
        cols: Option<String>,

        #[arg(long)]
        values: Option<String>,
    },

    /// Merge/join operations
    Merge {
        file1: String,
        file2: String,

        #[arg(long)]
        on: Option<String>,

        #[arg(long)]
        r#type: Option<String>,

        #[arg(long)]
        left_on: Option<String>,

        #[arg(long)]
        right_on: Option<String>,

        #[arg(long)]
        cross: bool,
    },

    /// Concatenate workbooks
    Concat {
        files: Vec<String>,

        #[arg(long, default_value = "rows")]
        axis: String,
    },

    /// Transpose a sheet
    Transpose {
        file: String,

        #[arg(long)]
        output: Option<String>,
    },

    /// Split a file
    Split {
        file: String,

        #[arg(long)]
        by: Option<String>,

        #[arg(long)]
        rows: Option<usize>,

        #[arg(long)]
        output_dir: Option<String>,
    },

    /// Sample rows
    Sample {
        file: String,

        #[arg(long)]
        n: Option<usize>,

        #[arg(long)]
        frac: Option<f64>,

        #[arg(long)]
        stratify: Option<String>,
    },

    /// Search and replace
    Search {
        file: String,

        #[arg(long)]
        pattern: String,

        #[arg(long)]
        r#type: Option<String>,

        #[arg(long)]
        regex: bool,

        #[arg(long)]
        case_sensitive: bool,

        #[arg(long)]
        whole_word: bool,
    },

    /// Replace content
    Replace {
        file: String,

        #[arg(long)]
        find: String,

        #[arg(long)]
        replace: String,

        #[arg(long)]
        r#type: Option<String>,

        #[arg(long)]
        dry_run: bool,
    },

    /// File information
    Info {
        file: String,

        #[arg(long)]
        detailed: bool,

        #[arg(long)]
        schema: bool,
    },

    /// Convert between formats
    Convert {
        input: String,
        output: String,

        #[arg(long)]
        sheet: Option<String>,

        #[arg(long)]
        delimiter: Option<String>,

        #[arg(long)]
        encoding: Option<String>,
    },

    /// Data cleaning operations
    Clean {
        file: String,

        #[arg(long)]
        command: String,

        #[arg(long)]
        range: Option<String>,

        #[arg(long)]
        columns: Option<String>,

        #[arg(long)]
        direction: Option<String>,

        #[arg(long)]
        fill: Option<String>,
    },

    /// Compare two files
    Diff {
        file1: String,
        file2: String,

        #[arg(long)]
        sheet: Option<String>,

        #[arg(long)]
        values_only: bool,

        #[arg(long)]
        tolerance: Option<f64>,
    },

    /// Batch operations
    Batch {
        glob: String,
        command: Vec<String>,
    },

    /// Detect encoding
    DetectEncoding {
        file: String,

        #[arg(long)]
        all: bool,
    },

    /// Detect language of content
    DetectLang {
        file: String,

        #[arg(long)]
        column: Option<String>,
    },

    /// Compare with another file
    Validate {
        file: String,

        #[arg(long)]
        reference: String,
    },

    /// Describe command schema (for AI Agent integration)
    Describe {
        #[arg(long)]
        all: bool,

        command: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SheetCmd {
    /// List all sheets
    List { file: String },

    /// Show sheet info
    Info {
        file: String,

        #[arg(long)]
        sheet: String,
    },

    /// Rename a sheet
    Rename {
        file: String,
        old: String,
        new: String,
    },

    /// Add a sheet
    Add {
        file: String,

        #[arg(long)]
        name: String,
    },

    /// Delete a sheet
    Delete {
        file: String,

        #[arg(long)]
        sheet: String,
    },

    /// Copy a sheet
    Copy {
        file: String,

        #[arg(long)]
        source: String,

        #[arg(long)]
        dest: String,
    },

    /// Move a sheet
    Move {
        file: String,

        #[arg(long)]
        sheet: String,

        #[arg(long)]
        index: usize,
    },

    /// Hide a sheet
    Hide {
        file: String,

        #[arg(long)]
        sheet: String,
    },

    /// Unhide a sheet
    Unhide {
        file: String,

        #[arg(long)]
        sheet: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum FormulaCmd {
    /// Read formulas
    Read {
        file: String,

        #[arg(long)]
        sheet: Option<String>,

        #[arg(long)]
        range: Option<String>,

        #[arg(long)]
        evaluate: bool,

        #[arg(long)]
        both: bool,
    },

    /// Set a formula
    Set {
        file: String,

        #[arg(long)]
        cell: Option<String>,

        #[arg(long)]
        range: Option<String>,

        #[arg(long)]
        formula: String,
    },

    /// Show formula dependencies
    Deps {
        file: String,

        #[arg(long)]
        cell: String,
    },

    /// Show formula dependents
    Dependents {
        file: String,

        #[arg(long)]
        cell: String,
    },

    /// List formula errors
    Errors {
        file: String,
    },

    /// Detect circular references
    Circular {
        file: String,
    },
}

pub fn build_cli() -> Cli {
    Cli::parse()
}
