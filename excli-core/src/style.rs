use serde::{Deserialize, Serialize};

/// Complete cell style definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellStyle {
    pub font: Option<Font>,
    pub fill: Option<Fill>,
    pub border: Option<Border>,
    pub alignment: Option<Alignment>,
    pub number_format: Option<String>,
    pub protection: Option<CellProtection>,
}

impl Default for CellStyle {
    fn default() -> Self {
        CellStyle {
            font: None,
            fill: None,
            border: None,
            alignment: None,
            number_format: None,
            protection: None,
        }
    }
}

impl CellStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_font(mut self, font: Font) -> Self {
        self.font = Some(font);
        self
    }

    pub fn with_fill(mut self, fill: Fill) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn with_border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }

    pub fn with_number_format(mut self, fmt: impl Into<String>) -> Self {
        self.number_format = Some(fmt.into());
        self
    }

    pub fn with_alignment(mut self, align: Alignment) -> Self {
        self.alignment = Some(align);
        self
    }

    pub fn bold() -> Self {
        CellStyle::new().with_font(Font::default().with_bold(true))
    }

    pub fn header() -> Self {
        CellStyle::new()
            .with_font(Font::default().with_bold(true).with_size(12.0))
            .with_fill(Fill::solid("#D9E1F2"))
            .with_border(Border::thin())
    }
}

/// Font definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Font {
    pub name: Option<String>,
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<Underline>,
    pub strikethrough: Option<bool>,
    pub color: Option<String>,
    pub superscript: Option<bool>,
    pub subscript: Option<bool>,
}

impl Default for Font {
    fn default() -> Self {
        Font {
            name: None,
            size: None,
            bold: None,
            italic: None,
            underline: None,
            strikethrough: None,
            color: None,
            superscript: None,
            subscript: None,
        }
    }
}

impl Font {
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_size(mut self, size: f64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = Some(bold);
        self
    }

    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = Some(italic);
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn with_underline(mut self, underline: Underline) -> Self {
        self.underline = Some(underline);
        self
    }
}

/// Underline style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Underline {
    Single,
    Double,
    SingleAccounting,
    DoubleAccounting,
}

/// Cell fill / background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub fill_type: FillType,
    pub fg_color: Option<String>,
    pub bg_color: Option<String>,
    pub pattern: Option<FillPattern>,
}

impl Fill {
    pub fn solid(color: impl Into<String>) -> Self {
        Fill {
            fill_type: FillType::Solid,
            fg_color: Some(color.into()),
            bg_color: None,
            pattern: None,
        }
    }

    pub fn none() -> Self {
        Fill {
            fill_type: FillType::None,
            fg_color: None,
            bg_color: None,
            pattern: None,
        }
    }
}

impl Default for Fill {
    fn default() -> Self {
        Fill::none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillType {
    None,
    Solid,
    Pattern,
    Gradient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillPattern {
    Solid,
    DarkGray,
    MediumGray,
    LightGray,
    Gray125,
    Gray0625,
    DarkHorizontal,
    DarkVertical,
    DarkDown,
    DarkUp,
    DarkGrid,
    DarkTrellis,
    LightHorizontal,
    LightVertical,
    LightDown,
    LightUp,
    LightGrid,
    LightTrellis,
}

/// Cell border.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Border {
    pub top: Option<BorderSide>,
    pub bottom: Option<BorderSide>,
    pub left: Option<BorderSide>,
    pub right: Option<BorderSide>,
    pub diagonal: Option<BorderSide>,
    pub diagonal_up: bool,
    pub diagonal_down: bool,
}

impl Border {
    pub fn thin() -> Self {
        let side = BorderSide {
            style: BorderStyle::Thin,
            color: Some("#000000".to_string()),
        };
        Border {
            top: Some(side.clone()),
            bottom: Some(side.clone()),
            left: Some(side.clone()),
            right: Some(side),
            diagonal: None,
            diagonal_up: false,
            diagonal_down: false,
        }
    }

    pub fn all(style: BorderStyle, color: Option<String>) -> Self {
        let side = BorderSide {
            style,
            color: color.clone(),
        };
        Border {
            top: Some(side.clone()),
            bottom: Some(side.clone()),
            left: Some(side.clone()),
            right: Some(side),
            diagonal: None,
            diagonal_up: false,
            diagonal_down: false,
        }
    }
}

impl Default for Border {
    fn default() -> Self {
        Border {
            top: None,
            bottom: None,
            left: None,
            right: None,
            diagonal: None,
            diagonal_up: false,
            diagonal_down: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BorderSide {
    pub style: BorderStyle,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderStyle {
    None,
    Thin,
    Medium,
    Thick,
    Double,
    Dotted,
    Dashed,
    Hair,
    DashDot,
    DashDotDot,
    SlantDashDot,
}

/// Cell alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alignment {
    pub horizontal: Option<HorizontalAlign>,
    pub vertical: Option<VerticalAlign>,
    pub wrap_text: bool,
    pub shrink_to_fit: bool,
    pub text_rotation: Option<u16>,
    pub indent: Option<u16>,
}

impl Default for Alignment {
    fn default() -> Self {
        Alignment {
            horizontal: None,
            vertical: None,
            wrap_text: false,
            shrink_to_fit: false,
            text_rotation: None,
            indent: None,
        }
    }
}

impl Alignment {
    pub fn center() -> Self {
        Alignment {
            horizontal: Some(HorizontalAlign::Center),
            vertical: Some(VerticalAlign::Center),
            ..Default::default()
        }
    }

    pub fn left() -> Self {
        Alignment {
            horizontal: Some(HorizontalAlign::Left),
            ..Default::default()
        }
    }

    pub fn right() -> Self {
        Alignment {
            horizontal: Some(HorizontalAlign::Right),
            ..Default::default()
        }
    }

    pub fn with_wrap(mut self, wrap: bool) -> Self {
        self.wrap_text = wrap;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterContinuous,
    Distributed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
    Justify,
    Distributed,
}

/// Cell protection settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellProtection {
    pub locked: bool,
    pub hidden: bool,
}

impl Default for CellProtection {
    fn default() -> Self {
        CellProtection {
            locked: true,
            hidden: false,
        }
    }
}

// Conditional formatting types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalFormat {
    pub range: String,
    pub priority: u32,
    pub rule: ConditionalFormatRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionalFormatRule {
    CellValue {
        operator: ConditionalOperator,
        value: String,
        style: CellStyle,
    },
    Formula {
        formula: String,
        style: CellStyle,
    },
    ColorScale {
        min_type: ScalePoint,
        mid_type: Option<ScalePoint>,
        max_type: ScalePoint,
        colors: (String, Option<String>, String),
    },
    DataBar {
        color: String,
        show_value: bool,
    },
    IconSet {
        icon_set: String,
        reverse: bool,
        show_value: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionalOperator {
    GreaterThan,
    LessThan,
    Between,
    Equal,
    NotEqual,
    ContainsText,
    NotContainsText,
    BeginsWith,
    EndsWith,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScalePoint {
    Min,
    Max,
    Percentile(u8),
    Percent(f64),
    Number(f64),
    Formula(String),
}

// Data validation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataValidation {
    pub range: String,
    pub validation_type: ValidationType,
    pub allow_blank: bool,
    pub show_error: bool,
    pub error_title: Option<String>,
    pub error_message: Option<String>,
    pub prompt_title: Option<String>,
    pub prompt_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationType {
    List {
        source: String,
    },
    Whole {
        between: Option<(i64, i64)>,
        min: Option<i64>,
        max: Option<i64>,
    },
    Decimal {
        between: Option<(f64, f64)>,
        min: Option<f64>,
        max: Option<f64>,
    },
    TextLength {
        min: Option<u32>,
        max: Option<u32>,
    },
    Date {
        min: Option<String>,
        max: Option<String>,
    },
    Time {
        min: Option<String>,
        max: Option<String>,
    },
    Custom {
        formula: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_style_default() {
        let style = CellStyle::default();
        assert!(style.font.is_none());
        assert!(style.fill.is_none());
    }

    #[test]
    fn test_cell_style_bold() {
        let style = CellStyle::bold();
        assert_eq!(style.font.as_ref().unwrap().bold, Some(true));
    }

    #[test]
    fn test_cell_style_header() {
        let style = CellStyle::header();
        let font = style.font.as_ref().unwrap();
        assert_eq!(font.bold, Some(true));
        assert_eq!(font.size, Some(12.0));
        assert!(style.fill.is_some());
        assert!(style.border.is_some());
    }

    #[test]
    fn test_font_builder() {
        let font = Font::default()
            .with_name("Arial")
            .with_size(14.0)
            .with_bold(true)
            .with_color("#FF0000");
        assert_eq!(font.name, Some("Arial".to_string()));
        assert_eq!(font.size, Some(14.0));
        assert_eq!(font.bold, Some(true));
        assert_eq!(font.color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_fill_solid() {
        let fill = Fill::solid("#FFFFFF");
        assert_eq!(fill.fill_type, FillType::Solid);
        assert_eq!(fill.fg_color, Some("#FFFFFF".to_string()));
    }

    #[test]
    fn test_alignment_center() {
        let align = Alignment::center();
        assert_eq!(align.horizontal, Some(HorizontalAlign::Center));
        assert_eq!(align.vertical, Some(VerticalAlign::Center));
    }

    #[test]
    fn test_border_thin() {
        let border = Border::thin();
        assert!(border.top.is_some());
        assert_eq!(border.top.as_ref().unwrap().style, BorderStyle::Thin);
    }

    #[test]
    fn test_cell_protection_default() {
        let protection = CellProtection::default();
        assert!(protection.locked);
        assert!(!protection.hidden);
    }

    #[test]
    fn test_conditional_format_creation() {
        let cf = ConditionalFormat {
            range: "A1:A100".to_string(),
            priority: 1,
            rule: ConditionalFormatRule::CellValue {
                operator: ConditionalOperator::GreaterThan,
                value: "100".to_string(),
                style: CellStyle::bold(),
            },
        };
        assert_eq!(cf.priority, 1);
        assert_eq!(cf.range, "A1:A100");
    }
}
