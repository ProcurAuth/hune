//! The typed model of format 3 (4.5 to 4.33).
//!
//! Each type has the members of its table, in table order. Only [`super::validate`] makes a
//! [`Document`] from bytes.

use crate::ids::{SourceId, Uid};

/// A closed list of texts (an enum of 4.2).
pub trait TextEnum: Sized + Copy + 'static {
    /// The permitted texts, in table order.
    const NAMES: &'static [&'static str];
    /// Reads a text. Another text gives `None`.
    fn parse(s: &str) -> Option<Self>;
    /// Gives the text of the value.
    fn as_str(self) -> &'static str;
}

macro_rules! text_enum {
    ($(#[$m:meta])* pub enum $name:ident { $($(#[$vm:meta])* $var:ident => $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($(#[$vm])* $var),+ }

        impl $name {
            /// Every value, in table order.
            pub const ALL: &'static [$name] = &[$($name::$var),+];
        }

        impl TextEnum for $name {
            const NAMES: &'static [&'static str] = &[$($text),+];
            fn parse(s: &str) -> Option<Self> {
                match s { $($text => Some($name::$var),)+ _ => None }
            }
            fn as_str(self) -> &'static str {
                match self { $($name::$var => $text),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(TextEnum::as_str(*self))
            }
        }
    };
}

text_enum! {
    /// The member `direction` of the `repeat` of a panel (4.13a).
    pub enum RepeatDirection { H => "h", V => "v" }
}
text_enum! {
    /// The member `cursor_sync` (4.5).
    pub enum CursorSync { Off => "off", Crosshair => "crosshair" }
}
text_enum! {
    /// The member `kind` of a dashboard link (4.5a).
    pub enum DashboardLinkKind { Link => "link", Dashboards => "dashboards" }
}
text_enum! {
    /// The property `axis_placement` (4.18).
    pub enum AxisPlacement { Auto => "auto", Left => "left", Right => "right", Hidden => "hidden" }
}
text_enum! {
    /// The property `line_dash` (4.18).
    pub enum LineDash { Solid => "solid", Dash => "dash", Dot => "dot" }
}
text_enum! {
    /// The property `transform` (4.18).
    pub enum SeriesTransform { None => "none", NegativeY => "negative_y" }
}
text_enum! {
    /// The property `axis_color` (4.18, wave H item P14): the colour of the y axis.
    pub enum AxisColor { Text => "text", Series => "series" }
}
text_enum! {
    /// The option `gradient_mode` of `timeseries` and `barchart` (4.28, 4.30, wave H item P10).
    pub enum GradientMode { None => "none", Opacity => "opacity", Hue => "hue", Scheme => "scheme" }
}
text_enum! {
    /// The option `bar_alignment` of `timeseries` (4.28, wave H item P14): the place of a bar against its time.
    pub enum BarAlignment { Before => "before", Center => "center", After => "after" }
}
text_enum! {
    /// The member `hide` of a variable (4.7).
    pub enum Hide { None => "none", Label => "label", Variable => "variable" }
}
text_enum! {
    /// The member `kind` of a variable (4.7).
    pub enum VariableTag {
        Query => "query",
        Custom => "custom",
        Text => "text",
        Interval => "interval",
        Constant => "constant",
        Adhoc => "adhoc",
    }
}
text_enum! {
    /// The member `refresh` of a `query` variable (4.8).
    pub enum VariableRefresh { Load => "load", TimeChange => "time_change" }
}
text_enum! {
    /// The member `sort` of a `query` variable (4.8).
    pub enum VariableSort {
        None => "none",
        AlphaAsc => "alpha_asc",
        AlphaDesc => "alpha_desc",
        NumAsc => "num_asc",
        NumDesc => "num_desc",
        AlphaCiAsc => "alpha_ci_asc",
        AlphaCiDesc => "alpha_ci_desc",
        NaturalAsc => "natural_asc",
        NaturalDesc => "natural_desc",
    }
}
text_enum! {
    /// The member `value_type` of a `text` variable (4.10).
    pub enum ValueType { Text => "text", Number => "number" }
}
text_enum! {
    /// The member `type` of a panel (4.13).
    pub enum PanelType {
        Table => "table",
        Timeseries => "timeseries",
        Stat => "stat",
        Barchart => "barchart",
        Piechart => "piechart",
        Xychart => "xychart",
        Bargauge => "bargauge",
        Text => "text",
        StateTimeline => "state_timeline",
        StatusHistory => "status_history",
        Heatmap => "heatmap",
        Histogram => "histogram",
        Logs => "logs",
        Gauge => "gauge",
        Geomap => "geomap",
    }
}
text_enum! {
    /// The member `options.mode` of a text panel (4.33a). `markdown` is the only value: HTML is text.
    pub enum TextPanelMode { Markdown => "markdown" }
}
text_enum! {
    /// The member `format` of a panel query (4.15).
    pub enum QueryFormat { Table => "table", TimeSeries => "time_series", Logs => "logs" }
}
text_enum! {
    /// The option `order` of a `logs` panel (4.33f).
    pub enum LogsOrder { NewestFirst => "newest_first", OldestFirst => "oldest_first" }
}
text_enum! {
    /// The member `by` of a matcher (4.17).
    pub enum MatchBy { Name => "name", Regex => "regex", Names => "names", Query => "query" }
}
text_enum! {
    /// The member `mode` of a matcher with `by` = `names` (4.17).
    pub enum NamesMode { Include => "include", Exclude => "exclude" }
}
text_enum! {
    /// The property `align` (4.18).
    pub enum Align { Auto => "auto", Left => "left", Center => "center", Right => "right" }
}
text_enum! {
    /// The member `cell.type` (4.18).
    pub enum CellType {
        Auto => "auto",
        ColorText => "color_text",
        ColorBackground => "color_background",
        Gauge => "gauge",
        Json => "json",
    }
}
text_enum! {
    /// The member `cell.mode` (4.18). `ColorBackground` takes `Gradient` or `Basic`. `Gauge` takes
    /// all three values.
    pub enum CellMode { Gradient => "gradient", Basic => "basic", Lcd => "lcd" }
}
text_enum! {
    /// The member `color.mode` (4.19). `Palette` is the one series palette (9.16).
    pub enum ColorMode {
        Fixed => "fixed",
        Thresholds => "thresholds",
        Palette => "palette",
        ContinuousGrYlRd => "continuous_gr_yl_rd",
        ContinuousRdYlGr => "continuous_rd_yl_gr",
        ContinuousBlYlRd => "continuous_bl_yl_rd",
        ContinuousYlRd => "continuous_yl_rd",
        ContinuousBlPu => "continuous_bl_pu",
        ContinuousYlBl => "continuous_yl_bl",
        ContinuousBlues => "continuous_blues",
        ContinuousReds => "continuous_reds",
        ContinuousGreens => "continuous_greens",
        ContinuousPurples => "continuous_purples",
    }
}
text_enum! {
    /// The member `thresholds.mode` (4.21).
    pub enum ThresholdMode { Absolute => "absolute", Percentage => "percentage" }
}
text_enum! {
    /// The kind of a mapping (4.22).
    pub enum MappingTag { Value => "value", Range => "range", Regex => "regex", Special => "special" }
}
text_enum! {
    /// The member `match` of a `special` mapping (4.22).
    pub enum SpecialMatch {
        Null => "null",
        Nan => "nan",
        NullNan => "null_nan",
        True => "true",
        False => "false",
        Empty => "empty",
    }
}
text_enum! {
    /// The kind of a transform (4.25, 4.25a).
    pub enum TransformTag {
        FilterByValue => "filter_by_value",
        GroupingToMatrix => "grouping_to_matrix",
        Organize => "organize",
        CalculateField => "calculate_field",
        SortBy => "sort_by",
        Limit => "limit",
        Merge => "merge",
        Reduce => "reduce",
        PartitionByValues => "partition_by_values",
        JoinByField => "join_by_field",
        FilterFieldsByName => "filter_fields_by_name",
        RenameByRegex => "rename_by_regex",
        ConvertFieldType => "convert_field_type",
    }
}
text_enum! {
    /// The member `mode` of a `calculate_field` transform (4.25a).
    pub enum CalcMode { Binary => "binary", ReduceRow => "reduce_row", Unary => "unary", Index => "index" }
}
text_enum! {
    /// The member `operator` of the object `binary` of a `calculate_field` transform (4.25a).
    pub enum BinaryOperator { Add => "add", Subtract => "subtract", Multiply => "multiply", Divide => "divide" }
}
text_enum! {
    /// The member `operator` of the object `unary` of a `calculate_field` transform (4.25a).
    pub enum UnaryOperator {
        Abs => "abs",
        Exp => "exp",
        Ln => "ln",
        Round => "round",
        Floor => "floor",
        Ceil => "ceil",
        Percent => "percent",
    }
}
text_enum! {
    /// The member `kind` of an operand of a `calculate_field` transform (4.25a).
    pub enum OperandKind { Field => "field", Number => "number" }
}
text_enum! {
    /// The member `end` of a `limit` transform (4.25a).
    pub enum LimitEnd { First => "first", Last => "last" }
}
text_enum! {
    /// The member `mode` of a `reduce` transform (4.25a).
    pub enum ReduceMode { SeriesToRows => "series_to_rows", ReduceFields => "reduce_fields" }
}
text_enum! {
    /// The member `mode` of a `join_by_field` transform (4.25a).
    pub enum JoinMode { Outer => "outer", Inner => "inner" }
}
text_enum! {
    /// The member `to` of a conversion of a `convert_field_type` transform (4.25a).
    pub enum ConvertTo { Number => "number", Text => "text", Time => "time", Boolean => "boolean" }
}
text_enum! {
    /// The member `mode` of a `filter_by_value` transform (4.25).
    pub enum FilterMode { Include => "include", Exclude => "exclude" }
}
text_enum! {
    /// The member `match` of a `filter_by_value` transform (4.25).
    pub enum FilterMatch { Any => "any", All => "all" }
}
text_enum! {
    /// The kind of a filter matcher (4.25).
    pub enum MatcherTag {
        Equal => "equal",
        NotEqual => "not_equal",
        Greater => "greater",
        GreaterOrEqual => "greater_or_equal",
        Lower => "lower",
        LowerOrEqual => "lower_or_equal",
        Range => "range",
        Regex => "regex",
        IsNull => "is_null",
        IsNotNull => "is_not_null",
    }
}
text_enum! {
    /// The member `empty_value` of a `grouping_to_matrix` transform (4.25).
    pub enum EmptyValue { Empty => "empty", Null => "null", True => "true", False => "false" }
}
text_enum! {
    /// The member `legend.placement` (4.26).
    pub enum LegendPlacement { Bottom => "bottom", Right => "right" }
}
text_enum! {
    /// The member `legend.mode` (4.26).
    pub enum LegendMode { List => "list", Table => "table" }
}
text_enum! {
    /// An item of `legend.values` (4.26). A `piechart` uses `Value` and `Percent` only. Every
    /// other panel type uses the other values: they are the values of `ReduceCalc`.
    pub enum LegendValue {
        Total => "total",
        Min => "min",
        Max => "max",
        Mean => "mean",
        Last => "last",
        Value => "value",
        Percent => "percent",
        LastNotNull => "last_not_null",
        First => "first",
        FirstNotNull => "first_not_null",
        Count => "count",
        Median => "median",
        StdDev => "std_dev",
        Variance => "variance",
        Range => "range",
        Diff => "diff",
        DiffPercent => "diff_percent",
        Delta => "delta",
        DistinctCount => "distinct_count",
        ChangeCount => "change_count",
    }
}
text_enum! {
    /// The member `legend.sort_by` (4.26): the column that sorts the legend table. `Name` is the
    /// series name. A `piechart` uses `Name`, `Value` and `Percent`. Every other panel type uses
    /// `Name` and the values of `ReduceCalc`.
    pub enum LegendSortBy {
        Name => "name",
        Total => "total",
        Min => "min",
        Max => "max",
        Mean => "mean",
        Last => "last",
        Value => "value",
        Percent => "percent",
        LastNotNull => "last_not_null",
        First => "first",
        FirstNotNull => "first_not_null",
        Count => "count",
        Median => "median",
        StdDev => "std_dev",
        Variance => "variance",
        Range => "range",
        Diff => "diff",
        DiffPercent => "diff_percent",
        Delta => "delta",
        DistinctCount => "distinct_count",
        ChangeCount => "change_count",
    }
}
text_enum! {
    /// The member `tooltip.mode` (4.26).
    pub enum TooltipMode { Single => "single", Multi => "multi", Hidden => "hidden" }
}
text_enum! {
    /// The member `tooltip.sort` (4.26): the order of the values in a `multi` tooltip.
    pub enum TooltipSort { None => "none", Asc => "asc", Desc => "desc" }
}
text_enum! {
    /// The member `reduce.calc` (4.26).
    pub enum ReduceCalc {
        Last => "last",
        LastNotNull => "last_not_null",
        First => "first",
        FirstNotNull => "first_not_null",
        Min => "min",
        Max => "max",
        Mean => "mean",
        Total => "total",
        Count => "count",
        Median => "median",
        StdDev => "std_dev",
        Variance => "variance",
        Range => "range",
        Diff => "diff",
        DiffPercent => "diff_percent",
        Delta => "delta",
        DistinctCount => "distinct_count",
        ChangeCount => "change_count",
    }
}
text_enum! {
    /// The option `draw` of a `timeseries` (4.28).
    pub enum TimeseriesDraw { Line => "line", Bars => "bars", Points => "points" }
}
text_enum! {
    /// The option `interpolation` of a `timeseries` (4.28).
    pub enum Interpolation {
        Linear => "linear",
        Smooth => "smooth",
        StepBefore => "step_before",
        StepAfter => "step_after",
    }
}
text_enum! {
    /// The option `show_points` of a `timeseries` (4.28).
    pub enum ShowPoints { Auto => "auto", Always => "always", Never => "never" }
}
text_enum! {
    /// The option `stacking` of a `timeseries` or a `barchart` (4.28, 4.30).
    pub enum Stacking { None => "none", Normal => "normal", Percent => "percent" }
}
text_enum! {
    /// The option `y_scale` of a `timeseries` (4.28).
    pub enum YScale { Linear => "linear", Log => "log" }
}
text_enum! {
    /// The option `thresholds_style` of a `timeseries` or a `barchart` (4.28, 4.30).
    pub enum ThresholdsStyle {
        Off => "off",
        Line => "line",
        Dashed => "dashed",
        Area => "area",
        LineArea => "line_area",
        DashedArea => "dashed_area",
    }
}
text_enum! {
    /// The option `color_mode` of a `stat` (4.29).
    pub enum StatColorMode { None => "none", Value => "value", Background => "background" }
}
text_enum! {
    /// The option `graph_mode` of a `stat` (4.29).
    pub enum GraphMode { None => "none", Area => "area" }
}
text_enum! {
    /// The option `text_mode` of a `stat` (4.29).
    pub enum TextMode {
        Auto => "auto",
        Value => "value",
        ValueAndName => "value_and_name",
        Name => "name",
        None => "none",
    }
}
text_enum! {
    /// The option `orientation` of a `stat` (4.29).
    pub enum StatOrientation { Horizontal => "horizontal", Vertical => "vertical", Auto => "auto" }
}
text_enum! {
    /// The option `justify` of a `stat` (4.29).
    pub enum StatJustify { Auto => "auto", Center => "center" }
}
text_enum! {
    /// The option `sizing` of a `gauge` (4.33e). `Manual` reads `min_width` and `min_height`.
    pub enum GaugeSizing { Auto => "auto", Manual => "manual" }
}
text_enum! {
    /// The option `percent_change_color` of a `stat` (4.29): the colour of the percent change.
    pub enum PercentChangeColor { Standard => "standard", Inverted => "inverted", SameAsValue => "same_as_value" }
}
text_enum! {
    /// The option `orientation` of a `bargauge` (4.33).
    pub enum Orientation { Vertical => "vertical", Horizontal => "horizontal" }
}
text_enum! {
    /// The option `orientation` of a `barchart` (4.30). `Auto` draws horizontal bars when the plot is narrower than it is high.
    pub enum BarchartOrientation { Vertical => "vertical", Horizontal => "horizontal", Auto => "auto" }
}
text_enum! {
    /// The option `show_values` of a `barchart`, a `state_timeline` or a `status_history` (4.30, 4.33b).
    pub enum ShowValues { Auto => "auto", Always => "always", Never => "never" }
}
text_enum! {
    /// The option `pie_type` of a `piechart` (4.31).
    pub enum PieType { Pie => "pie", Donut => "donut" }
}
text_enum! {
    /// An item of `labels` of a `piechart` (4.31).
    pub enum PieLabel { Name => "name", Value => "value", Percent => "percent" }
}
text_enum! {
    /// The option `draw` of an `xychart` (4.32).
    pub enum XyDraw { Points => "points", Lines => "lines", PointsAndLines => "points_and_lines" }
}
text_enum! {
    /// The option `display_mode` of a `bargauge` (4.33).
    pub enum BarGaugeMode { Basic => "basic", Gradient => "gradient", Lcd => "lcd" }
}
text_enum! {
    /// The option `align_values` of a `state_timeline` (4.33b).
    pub enum AlignValues { Left => "left", Center => "center", Right => "right" }
}
text_enum! {
    /// The member `mode` of `x_buckets` and `y_buckets` of a `heatmap` (4.33c).
    pub enum BucketMode { Count => "count", Size => "size" }
}
text_enum! {
    /// The member `scale` of `y_buckets` of a `heatmap` (4.33c).
    pub enum HeatScale { Linear => "linear", Log2 => "log2", Log10 => "log10" }
}
text_enum! {
    /// The option `y_axis` of a `heatmap` (4.33c).
    pub enum HeatYAxis { Left => "left", Hidden => "hidden" }
}
text_enum! {
    /// The member `color.mode` of a `heatmap` (4.33c).
    pub enum HeatColorMode { Scheme => "scheme", Opacity => "opacity" }
}
text_enum! {
    /// The member `color.scheme` of a `heatmap`: the closed list of 10 schemes (4.33c, 9.19e).
    pub enum HeatScheme {
        Oranges => "oranges",
        Blues => "blues",
        Greens => "greens",
        Greys => "greys",
        Purples => "purples",
        Reds => "reds",
        YlOrRd => "yl_or_rd",
        RdYlGn => "rd_yl_gn",
        Spectral => "spectral",
        Viridis => "viridis",
    }
}
text_enum! {
    /// The member `color.scale` of a `heatmap` (4.33c).
    pub enum HeatColorScale { Exponential => "exponential", Linear => "linear" }
}

/// The typed document of format 3 (4.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The uid (4.3). It equals the location of the document.
    pub uid: Uid,
    /// The title: 1 to 256 characters.
    pub title: String,
    /// The description: 0 to 4096 characters.
    pub description: String,
    /// The tags: 0 to 32 unique texts.
    pub tags: Vec<String>,
    /// The default time range (4.6).
    pub time: TimeSettings,
    /// The cursor synchronisation of the time charts.
    pub cursor_sync: CursorSync,
    /// The dashboard links (4.5a): 0 to `MAX_DASHBOARD_LINKS`.
    pub links: Vec<DashboardLink>,
    /// The variables (4.7): 0 to 64.
    pub variables: Vec<Variable>,
    /// The annotations (4.5b): 0 to `MAX_ANNOTATIONS`. The canonical form leaves out an empty array.
    pub annotations: Vec<Annotation>,
    /// The sections (4.12): 1 to 50.
    pub sections: Vec<Section>,
}

/// One annotation (4.5b): a saved query that gives time marks for the time series panels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// 1 to 256 characters. It is unique in the document.
    pub name: String,
    /// The source of the query. It MUST be a ClickHouse source (the save checks the kind, 7.18).
    pub source: SourceId,
    /// The SQL lines (4.16).
    pub sql: Vec<String>,
    /// A colour of 4.19.
    pub color: String,
    /// `false`: the page runs no query for the annotation. Default `true`.
    pub enabled: bool,
    /// The ids of the panels that show the marks. `None`: each time series panel. `Some`: 1 to
    /// `MAX_ANNOTATION_PANELS` unique ids, in the order that the author wrote.
    pub panels: Option<Vec<u32>>,
}

/// The member `time` (4.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSettings {
    /// The default start: a time text (4.4).
    pub from: String,
    /// The default end: a time text (4.4).
    pub to: String,
    /// The time zone of the display (4.6).
    pub timezone: Timezone,
    /// `None`: no automatic refresh. Else one value of `REFRESH_CHOICES`.
    pub refresh: Option<String>,
}

/// The member `time.timezone` (4.6): `browser`, `utc` or a zone name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Timezone {
    /// The time zone of the browser.
    Browser,
    /// UTC.
    Utc,
    /// An IANA zone name, such as `Europe/Paris`. The format checks the grammar only.
    Named(String),
}

impl Timezone {
    /// Gives the text of the member.
    pub fn as_str(&self) -> &str {
        match self {
            Timezone::Browser => "browser",
            Timezone::Utc => "utc",
            Timezone::Named(name) => name,
        }
    }
}

/// A dashboard link (4.5a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DashboardLink {
    /// The kind `link`: a link to one URL.
    Link {
        /// 1 to 256 characters.
        title: String,
        /// A URL template: 1 to 2048 bytes.
        url: String,
        /// `true`: the link opens a new tab.
        target_blank: bool,
        /// `true`: the page adds the selection of each variable.
        include_vars: bool,
        /// `true`: the page adds the time range.
        keep_time: bool,
    },
    /// The kind `dashboards`: a link to each dashboard that has all the tags.
    Dashboards {
        /// 0 to 256 characters. 1 to 256 with `as_dropdown`.
        title: String,
        /// 0 to `MAX_LINK_TAGS` unique tags.
        tags: Vec<String>,
        /// `true`: one menu with one item for each dashboard.
        as_dropdown: bool,
        /// `true`: the link opens a new tab.
        target_blank: bool,
        /// `true`: the page adds the selection of each variable.
        include_vars: bool,
        /// `true`: the page adds the time range.
        keep_time: bool,
    },
}

impl DashboardLink {
    /// Gives the value of the member `kind`.
    pub fn kind(&self) -> DashboardLinkKind {
        match self {
            DashboardLink::Link { .. } => DashboardLinkKind::Link,
            DashboardLink::Dashboards { .. } => DashboardLinkKind::Dashboards,
        }
    }
}

/// A variable (4.7).
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    /// The name (4.3).
    pub name: String,
    /// The label: 1 to 256 characters. `None`: the bar shows the name.
    pub label: Option<String>,
    /// What the bar shows.
    pub hide: Hide,
    /// The members of the kind.
    pub kind: VariableKind,
    /// The place of the control in the grid (4.7a). `None`: the control stays in the variables bar.
    pub placement: Option<Placement>,
}

/// The member `placement` of a variable (4.7a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The index of the section in `sections`, from 0.
    pub section: u32,
    /// The place and the size in the grid of that section (4.14).
    pub grid: Grid,
    /// 1 to 256 characters. `None`: the frame shows the label of the variable.
    pub title: Option<String>,
}

/// The members of the six variable kinds (4.8 to 4.10c).
#[derive(Debug, Clone, PartialEq)]
pub enum VariableKind {
    /// Kind `query` (4.8).
    Query {
        /// The data source.
        source: SourceId,
        /// The SQL lines (4.16).
        sql: Vec<String>,
        /// `true`: the viewer can select more than one value.
        multi: bool,
        /// `true`: the control has the item "All".
        include_all: bool,
        /// The SQL text that "All" gives. It is `None` when `include_all` is `false`.
        all_value: Option<String>,
        /// When the options load.
        refresh: VariableRefresh,
        /// The order of the options.
        sort: VariableSort,
        /// A pattern of the Grafana form (4.24), or `None`.
        regex: Option<String>,
        /// The default selection (4.11).
        default: Vec<String>,
        /// `true`: the variable is open: a person can also type a value (4.7, 4.8). The default is `false`.
        allow_custom_value: bool,
    },
    /// Kind `custom` (4.9).
    Custom {
        /// The options: 1 to 10,000.
        options: Vec<CustomOption>,
        /// `true`: the viewer can select more than one value.
        multi: bool,
        /// `true`: the control has the item "All".
        include_all: bool,
        /// The SQL text that "All" gives. It is `None` when `include_all` is `false`.
        all_value: Option<String>,
        /// The default selection (4.11).
        default: Vec<String>,
        /// `true`: the variable is open: each value is free text for the server (4.7, 4.9). The default is `false`.
        allow_custom_value: bool,
    },
    /// Kind `text` (4.10).
    Text {
        /// Free text or a decimal number.
        value_type: ValueType,
        /// The default value.
        default: String,
    },
    /// Kind `interval` (4.10a).
    Interval {
        /// The items: 1 to `MAX_INTERVALS`. Each one is an interval text of 365 days or less, and it is unique.
        intervals: Vec<String>,
        /// `true`: the control has the option "Auto" before the items.
        auto: bool,
        /// The step count of "Auto": 1 to 1,000.
        auto_count: u32,
        /// The smallest interval of "Auto": one of the 30 texts of the table of 7.4a.
        auto_min: String,
        /// An item of `intervals`, or `$__auto` when `auto` is `true`. `None`: the first option of the control.
        default: Option<String>,
    },
    /// Kind `constant` (4.10b).
    Constant {
        /// The text of the document: 0 to 1,024 bytes, no control character. The bar never shows it.
        value: String,
    },
    /// Kind `adhoc` (4.10c): the ad hoc filters of one ClickHouse source. The document holds no filter.
    Adhoc {
        /// The ClickHouse source of the filters. A document has one `adhoc` variable for each source at most.
        source: SourceId,
        /// The keys: 1 to `MAX_ADHOC_KEYS` unique plain column names. The checks at save compare them with the
        /// configured list of the source (7.18).
        keys: Vec<String>,
    },
}

impl VariableKind {
    /// Gives the kind tag of the variable.
    pub fn tag(&self) -> VariableTag {
        match self {
            VariableKind::Query { .. } => VariableTag::Query,
            VariableKind::Custom { .. } => VariableTag::Custom,
            VariableKind::Text { .. } => VariableTag::Text,
            VariableKind::Interval { .. } => VariableTag::Interval,
            VariableKind::Constant { .. } => VariableTag::Constant,
            VariableKind::Adhoc { .. } => VariableTag::Adhoc,
        }
    }

    /// Tells if the variable is open: the kind is `query` or `custom` and `allow_custom_value` is `true` (4.7).
    pub fn is_open(&self) -> bool {
        matches!(
            self,
            VariableKind::Query { allow_custom_value: true, .. }
                | VariableKind::Custom { allow_custom_value: true, .. }
        )
    }
}

/// An option of a `custom` variable (4.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomOption {
    /// The text that the control shows: 1 to 256 characters.
    pub text: String,
    /// The value that the SQL gets: 1 to 1024 bytes. It is unique.
    pub value: String,
}

/// A section: a header of rows and its panels (4.12).
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// The title: 1 to 256 characters. `None`: no header. Only the first section can have `None`.
    pub title: Option<String>,
    /// `true`: the section shows its header only.
    pub collapsed: bool,
    /// The panels: 0 to 200.
    pub panels: Vec<Panel>,
    /// The repeat of the section by a variable (4.13a). `None`: the section does not repeat.
    pub repeat: Option<SectionRepeat>,
}

/// The member `repeat` of a section (4.13a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionRepeat {
    /// The name of a `query` or `custom` variable that has `multi` equal to `true`.
    pub variable: String,
}

/// The member `repeat` of a panel (4.13a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRepeat {
    /// The name of a `query` or `custom` variable that has `multi` equal to `true`.
    pub variable: String,
    /// `h`: the copies share the width of the section. `v`: the copies are one under the other.
    pub direction: RepeatDirection,
    /// 1 to 24. It is valid with `direction` equal to `h` only. `None`: the page uses 4.
    pub max_per_row: Option<u32>,
}

/// A panel (4.13).
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    /// The id: 1 to 2147483647. It is unique in the document.
    pub id: u32,
    /// The type of the panel.
    pub panel_type: PanelType,
    /// The title: 0 to 256 characters.
    pub title: String,
    /// The description: 0 to 4096 characters.
    pub description: String,
    /// The panel has no background and no border at rest (4.13).
    pub transparent: bool,
    /// The place in the grid.
    pub grid: Grid,
    /// The data of the panel. A text panel has `PanelData::None` (4.33a).
    pub data: PanelData,
    /// The transforms: 0 to 16. A text panel has none.
    pub transforms: Vec<Transform>,
    /// The field defaults and overrides. A text panel has the empty set.
    pub fields: Fields,
    /// The options of the type.
    pub options: PanelOptions,
    /// The links of the header: 0 to 16.
    pub links: Vec<Link>,
    /// The repeat of the panel by a variable (4.13a). `None`: the panel does not repeat.
    pub repeat: Option<PanelRepeat>,
}

/// The member `grid` (4.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    /// The first column: 0 to 23.
    pub x: u32,
    /// The first row unit: 0 to 10,000.
    pub y: u32,
    /// The width in columns: 1 to 24.
    pub w: u32,
    /// The height in row units: 1 to 200.
    pub h: u32,
}

impl Grid {
    /// Tells if the two rectangles `[x, x + w) x [y, y + h)` have a common point.
    pub fn overlaps(&self, other: &Grid) -> bool {
        let end = |a: u32, b: u32| a.saturating_add(b);
        self.x < end(other.x, other.w)
            && other.x < end(self.x, self.w)
            && self.y < end(other.y, other.h)
            && other.y < end(self.y, self.h)
    }
}

/// The member `data` of a panel (4.15).
#[derive(Debug, Clone, PartialEq)]
pub enum PanelData {
    /// The panel runs its own SQL.
    Query(QueryData),
    /// The panel has no query: the type `text` (4.33a). The document has no member `data` for it.
    None,
    /// The panel uses the frame of another panel.
    Reuse {
        /// The id of the source panel. It has `PanelData::Query`.
        panel: u32,
        /// `true`: the transforms of the source panel apply first.
        with_transforms: bool,
    },
}

/// The member `data.query` (4.15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryData {
    /// The data source.
    pub source: SourceId,
    /// The frame format.
    pub format: QueryFormat,
    /// The SQL lines (4.16).
    pub sql: Vec<String>,
    /// A duration from `1ms` to `1d`. `None`: 1 ms.
    pub min_interval: Option<String>,
    /// 1 to 10,000. `None`: the width of the plot.
    pub max_data_points: Option<u32>,
    /// A relative time text that starts with `now-` or `now/` (4.15). `None`: the dashboard range.
    pub time_from: Option<String>,
    /// A shift: a count and a unit (4.15). `None`: no shift.
    pub time_shift: Option<String>,
    /// `true`: the title bar has no note about the time override. Default `false`.
    pub hide_time_override: bool,
    /// The ref of the query in the panel (4.15a). Absent in the document: `A` (`DEFAULT_QUERY_REF`).
    pub query_ref: String,
    /// `true`: the query does not run (4.15a, rule 3). Default `false`.
    pub hide: bool,
    /// The other queries of the panel (4.15a): 0 to `MAX_QUERIES_PER_PANEL` minus 1 items. The canonical form
    /// leaves out an empty list.
    pub extra_queries: Vec<ExtraQuery>,
}

/// The ref of the first query of a panel when the document has no `ref` (4.3, 4.15a).
pub const DEFAULT_QUERY_REF: &str = "A";

/// One item of `data.extra_queries` (4.15a). The members of the panel stay in `data.query` (rule 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraQuery {
    /// The ref (4.3). It is unique in the panel.
    pub query_ref: String,
    /// The data source of the query.
    pub source: SourceId,
    /// The frame format.
    pub format: QueryFormat,
    /// The SQL lines (4.16).
    pub sql: Vec<String>,
    /// `true`: the query does not run. Default `false`.
    pub hide: bool,
}

/// One query of a panel, as `data.query` or as an item of `data.extra_queries` (4.15a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryView<'a> {
    /// The place of the query: 0 for `data.query`, then 1 for the first item of `extra_queries`.
    pub position: usize,
    /// The ref of the query.
    pub query_ref: &'a str,
    /// The data source of the query.
    pub source: &'a SourceId,
    /// The frame format.
    pub format: QueryFormat,
    /// The SQL lines.
    pub sql: &'a [String],
    /// `true`: the query does not run.
    pub hide: bool,
}

impl QueryView<'_> {
    /// Gives the SQL text: the lines joined with one U+000A between two lines (4.16).
    pub fn sql_text(&self) -> String {
        self.sql.join("\n")
    }
}

impl QueryData {
    /// Gives each query of the panel in order: `data.query`, then the items of `extra_queries` (4.15a).
    pub fn queries(&self) -> Vec<QueryView<'_>> {
        let first = QueryView {
            position: 0,
            query_ref: &self.query_ref,
            source: &self.source,
            format: self.format,
            sql: &self.sql,
            hide: self.hide,
        };
        let rest = self.extra_queries.iter().enumerate().map(|(i, q)| QueryView {
            position: i + 1,
            query_ref: &q.query_ref,
            source: &q.source,
            format: q.format,
            sql: &q.sql,
            hide: q.hide,
        });
        std::iter::once(first).chain(rest).collect()
    }

    /// Finds a query by its ref (6.20). `None` gives the first query, `data.query`.
    pub fn find_query(&self, query_ref: Option<&str>) -> Option<QueryView<'_>> {
        let all = self.queries();
        match query_ref {
            None => all.into_iter().next(),
            Some(r) => all.into_iter().find(|q| q.query_ref == r),
        }
    }

    /// Gives the number of queries of the panel, hidden queries included.
    pub fn query_count(&self) -> usize {
        1 + self.extra_queries.len()
    }

    /// Gives the SQL text: the lines joined with one U+000A between two lines (4.16).
    pub fn sql_text(&self) -> String {
        self.sql.join("\n")
    }

    /// Gives the minimum interval in milliseconds. `None` gives `None`.
    pub fn min_interval_ms(&self) -> Option<u64> {
        self.min_interval.as_deref().and_then(super::time_text::parse_duration_ms)
    }
}

/// The member `fields` (4.17).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fields {
    /// The property set of each field.
    pub defaults: PropertySet,
    /// The overrides: 0 to 512.
    pub overrides: Vec<Override>,
}

/// An override (4.17).
#[derive(Debug, Clone, PartialEq)]
pub struct Override {
    /// The matcher (member `match`).
    pub matcher: FieldMatcher,
    /// The property set (member `set`). It has 1 member or more.
    pub set: PropertySet,
}

/// The matcher of an override (4.17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldMatcher {
    /// `by` = `name`: the field name.
    Name(String),
    /// `by` = `regex`: a pattern of the Grafana form (4.24).
    Regex(String),
    /// `by` = `names`: a list of field names.
    Names {
        /// 1 to `MAX_MATCHER_NAMES` field names. Each name has 1 to 1024 bytes.
        names: Vec<String>,
        /// `include`: the field matches when its name is in the list. `exclude`: it matches when
        /// its name is not in the list.
        mode: NamesMode,
    },
    /// `by` = `query`: the ref of a query of the panel (4.17, 4.15a).
    Query(String),
}

impl FieldMatcher {
    /// Gives the value of the member `by`.
    pub fn by(&self) -> MatchBy {
        match self {
            FieldMatcher::Name(_) => MatchBy::Name,
            FieldMatcher::Regex(_) => MatchBy::Regex,
            FieldMatcher::Names { .. } => MatchBy::Names,
            FieldMatcher::Query(_) => MatchBy::Query,
        }
    }
}

/// A property set (4.18). A member that is `None` is not set.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropertySet {
    /// A unit (4.20).
    pub unit: Option<String>,
    /// 0 to 20.
    pub decimals: Option<u8>,
    /// The low end of the scale.
    pub min: Option<f64>,
    /// The high end of the scale.
    pub max: Option<f64>,
    /// The colour mode.
    pub color: Option<ColorSpec>,
    /// The thresholds.
    pub thresholds: Option<Thresholds>,
    /// The mappings: 1 to 256.
    pub mappings: Option<Vec<Mapping>>,
    /// The data links: 1 to 16.
    pub links: Option<Vec<Link>>,
    /// The name that the panel shows for the field.
    pub display_name: Option<String>,
    /// The text for a value that is absent. Only in `fields.defaults`.
    pub no_value: Option<String>,
    /// The column width in CSS pixels: 10 to 4000. Table only.
    pub width: Option<u32>,
    /// The alignment. Table only.
    pub align: Option<Align>,
    /// The cell type. Table only.
    pub cell: Option<Cell>,
    /// `true`: the column header has a value filter. Table only.
    pub filterable: Option<bool>,
    /// `true`: the table does not show the column. Table only.
    pub hidden: Option<bool>,
    /// The footer calculations of the column. Table only.
    pub footer: Option<Footer>,
    /// `true`: each cell of the column has the button "Inspect value". Table only.
    pub inspect: Option<bool>,
    /// Where the series does not show.
    pub hide_from: Option<HideFrom>,
    /// The side of the y axis. Timeseries only.
    pub axis_placement: Option<AxisPlacement>,
    /// The title of the y axis: 1 to 256 characters. Timeseries and barchart.
    pub axis_label: Option<String>,
    /// The soft low end of the y axis. Timeseries and barchart.
    pub soft_min: Option<f64>,
    /// The soft high end of the y axis. Timeseries and barchart.
    pub soft_max: Option<f64>,
    /// The grid lines of the value axis: `false` removes them, `true` and `None` (Auto) draw them. Barchart only.
    pub axis_grid: Option<bool>,
    /// The style of the series. Only in an override. Timeseries only.
    pub draw: Option<TimeseriesDraw>,
    /// 0 to 10. Only in an override. Timeseries only.
    pub line_width: Option<u8>,
    /// 0 to 100. Only in an override. Timeseries only.
    pub fill_opacity: Option<u8>,
    /// The dash of the line. Timeseries only.
    pub line_dash: Option<LineDash>,
    /// When the series shows its points. Only in an override. Timeseries only.
    pub show_points: Option<ShowPoints>,
    /// 1 to 40. Only in an override. Timeseries only.
    pub point_size: Option<u8>,
    /// The transform of the values. Timeseries only.
    pub transform: Option<SeriesTransform>,
    /// The dash array: 1 to 8 integers from 0 to 100 (device pixels). Timeseries only (wave H, P14).
    pub line_dash_array: Option<Vec<u8>>,
    /// `true`: the y axis has 0 in its middle. Timeseries and barchart (P14).
    pub axis_centered_zero: Option<bool>,
    /// The width of the y axis: 0 to 500 CSS pixels. Timeseries and barchart (P14).
    pub axis_width: Option<u16>,
    /// `true`: the y axis has a line and ticks. Timeseries and barchart (P14).
    pub axis_border: Option<bool>,
    /// The colour of the y axis. Timeseries and barchart (P14).
    pub axis_color: Option<AxisColor>,
}

impl PropertySet {
    /// Tells if no member is set.
    pub fn is_empty(&self) -> bool {
        *self == PropertySet::default()
    }
}

/// The member `cell` of a property set (4.18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The cell type.
    pub cell_type: CellType,
    /// A mode exactly when `cell_type` is `ColorBackground` or `Gauge`.
    pub mode: Option<CellMode>,
}

/// The member `footer` of a property set (4.18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footer {
    /// 1 to `MAX_FOOTER_REDUCERS` calculations, no two equal. Each one gives one footer row.
    pub reducers: Vec<ReduceCalc>,
}

/// The member `hide_from` of a property set (4.18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HideFrom {
    /// `true`: the legend does not list the series.
    pub legend: bool,
    /// `true`: the tooltip does not show the series.
    pub tooltip: bool,
    /// `true`: the chart does not draw the series.
    pub viz: bool,
}

/// The member `color` of a property set (4.19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorSpec {
    /// The colour mode.
    pub mode: ColorMode,
    /// A colour exactly when `mode` is `Fixed`.
    pub fixed: Option<String>,
}

/// The member `thresholds` (4.21).
#[derive(Debug, Clone, PartialEq)]
pub struct Thresholds {
    /// What a step value is.
    pub mode: ThresholdMode,
    /// The steps: 1 to 32.
    pub steps: Vec<ThresholdStep>,
}

/// A threshold step (4.21).
#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdStep {
    /// `None` for the base step (the first step). A number for each other step.
    pub value: Option<f64>,
    /// A colour (4.19).
    pub color: String,
}

/// A mapping (4.22).
#[derive(Debug, Clone, PartialEq)]
pub enum Mapping {
    /// The display text equals `value`.
    Value {
        /// 0 to 1024 bytes.
        value: String,
        /// The new text, or `None`.
        text: Option<String>,
        /// The new colour, or `None`.
        color: Option<String>,
    },
    /// The number is in a range.
    Range {
        /// The low bound, or `None`.
        from: Option<f64>,
        /// The high bound, or `None`.
        to: Option<f64>,
        /// The new text, or `None`.
        text: Option<String>,
        /// The new colour, or `None`.
        color: Option<String>,
    },
    /// A pattern of the Grafana form matches the display text.
    Regex {
        /// The pattern.
        pattern: String,
        /// The new text, or `None`.
        text: Option<String>,
        /// The new colour, or `None`.
        color: Option<String>,
    },
    /// A special value.
    Special {
        /// The special value.
        matches: SpecialMatch,
        /// The new text, or `None`.
        text: Option<String>,
        /// The new colour, or `None`.
        color: Option<String>,
    },
}

/// A panel link or a data link (4.23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// 1 to 256 characters.
    pub title: String,
    /// A URL template: 1 to 2048 bytes.
    pub url: String,
    /// `true`: the link opens a new tab.
    pub target_blank: bool,
}

/// A transform (4.25).
#[derive(Debug, Clone, PartialEq)]
pub enum Transform {
    /// Kind `filter_by_value`.
    FilterByValue {
        /// Keep or remove the rows that pass.
        mode: FilterMode,
        /// `any` or `all`.
        match_mode: FilterMatch,
        /// The filters: 1 to 32.
        filters: Vec<Filter>,
    },
    /// Kind `grouping_to_matrix`.
    GroupingToMatrix {
        /// The field whose values become the columns.
        column_field: String,
        /// The field whose values become the rows.
        row_field: String,
        /// The field whose values fill the cells.
        value_field: String,
        /// The value of a cell without data.
        empty_value: EmptyValue,
    },
    /// Kind `organize` (4.25a).
    Organize {
        /// The fields that the transform removes: 0 to `MAX_TRANSFORM_ITEMS` current names.
        hide: Vec<String>,
        /// The fields that come first, in this order: 0 to `MAX_TRANSFORM_ITEMS` current names.
        order: Vec<String>,
        /// The renames: 0 to `MAX_TRANSFORM_ITEMS` items with different fields.
        rename: Vec<Rename>,
    },
    /// Kind `calculate_field` (4.25a). The member `mode` is the mode of `calc`.
    CalculateField {
        /// The name of the new field: 1 to 256 characters. `None`: the default name (9.21a, rule 3).
        alias: Option<String>,
        /// `true`: the result holds the first time field and the new field only.
        replace_fields: bool,
        /// The mode and the object of the mode.
        calc: Calc,
    },
    /// Kind `sort_by` (4.25a).
    SortBy {
        /// The sort keys: 1 to `MAX_SORT_KEYS`. The first key sorts first.
        sort: Vec<SortKey>,
    },
    /// Kind `limit` (4.25a).
    Limit {
        /// The number of rows to keep: 0 to 1,000,000.
        count: u32,
        /// Which rows to keep.
        end: LimitEnd,
    },
    /// Kind `merge` (4.25a): no member after `kind`.
    Merge,
    /// Kind `reduce` (4.25a).
    Reduce {
        /// One row for each field, or one field for each field and reducer.
        mode: ReduceMode,
        /// The calculations: 1 to 18 values, no two equal.
        reducers: Vec<ReduceCalc>,
    },
    /// Kind `partition_by_values` (4.25a).
    PartitionByValues {
        /// The key fields: 1 to 4 names, no two equal.
        fields: Vec<String>,
        /// `true`: the key fields stay in each new frame.
        keep_fields: bool,
    },
    /// Kind `join_by_field` (4.25a).
    JoinByField {
        /// The `name` of the key field. `None`: the first field of the kind `time` of each frame.
        field: Option<String>,
        /// All keys, or the keys that all frames have.
        mode: JoinMode,
    },
    /// Kind `filter_fields_by_name` (4.25a).
    FilterFieldsByName {
        /// The fields that pass. Both parts empty: all fields pass.
        include: NameFilter,
        /// The fields that do not stay. Both parts empty: no field is removed.
        exclude: NameFilter,
    },
    /// Kind `rename_by_regex` (4.25a).
    RenameByRegex {
        /// A pattern of the Grafana form (4.24): 1 to 1024 bytes.
        regex: String,
        /// The replacement text of the JavaScript `replace`: 0 to 256 characters.
        rename_pattern: String,
    },
    /// Kind `convert_field_type` (4.25a).
    ConvertFieldType {
        /// The conversions: 1 to 16 items with different fields.
        conversions: Vec<Conversion>,
    },
}

/// A rename of an `organize` transform (4.25a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rename {
    /// The current name of the field: 1 to 1024 bytes.
    pub field: String,
    /// The new name: 1 to 256 characters.
    pub name: String,
}

/// A conversion of a `convert_field_type` transform (4.25a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    /// The current name of the field: 1 to 1024 bytes.
    pub field: String,
    /// The new type.
    pub to: ConvertTo,
}

/// The member `include` or `exclude` of a `filter_fields_by_name` transform (4.25a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameFilter {
    /// 0 to `MAX_TRANSFORM_ITEMS` current names, no two equal.
    pub names: Vec<String>,
    /// A pattern of the Grafana form (4.24), or `None`.
    pub pattern: Option<String>,
}

/// An operand of the mode `binary` (4.25a).
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// A field: its current name, 1 to 1024 bytes.
    Field(String),
    /// A constant number.
    Number(f64),
}

/// The mode of a `calculate_field` transform with the object of the mode (4.25a).
#[derive(Debug, Clone, PartialEq)]
pub enum Calc {
    /// Mode `binary`: the object `binary`.
    Binary {
        /// The left operand.
        left: Operand,
        /// The right operand.
        right: Operand,
        /// The operator.
        operator: BinaryOperator,
    },
    /// Mode `reduce_row`: the object `reduce`.
    ReduceRow {
        /// The calculation of 4.26.
        reducer: ReduceCalc,
        /// The fields to reduce: 0 to `MAX_TRANSFORM_ITEMS` names. Empty: all fields of the type number.
        include: Vec<String>,
    },
    /// Mode `unary`: the object `unary`.
    Unary {
        /// The operator.
        operator: UnaryOperator,
        /// The current name of the field: 1 to 1024 bytes.
        field: String,
    },
    /// Mode `index`: the object `index`.
    Index {
        /// `true`: the values are i/n and not 0 to n-1.
        as_percentile: bool,
    },
}

impl Calc {
    /// The value of the member `mode`.
    pub fn mode(&self) -> CalcMode {
        match self {
            Calc::Binary { .. } => CalcMode::Binary,
            Calc::ReduceRow { .. } => CalcMode::ReduceRow,
            Calc::Unary { .. } => CalcMode::Unary,
            Calc::Index { .. } => CalcMode::Index,
        }
    }
}

/// A filter of `filter_by_value` (4.25).
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// The display name of the field: 1 to 1024 bytes.
    pub field: String,
    /// The matcher.
    pub matcher: FilterMatcher,
}

/// A number or a text (the `value` of `equal` and `not_equal`).
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    /// A number.
    Number(f64),
    /// A text of 0 to 1024 bytes.
    Text(String),
}

/// The matcher of a filter (4.25).
#[derive(Debug, Clone, PartialEq)]
pub enum FilterMatcher {
    /// Equals the value.
    Equal(Scalar),
    /// Does not equal the value.
    NotEqual(Scalar),
    /// Is greater than the value.
    Greater(f64),
    /// Is at least the value.
    GreaterOrEqual(f64),
    /// Is less than the value.
    Lower(f64),
    /// Is at most the value.
    LowerOrEqual(f64),
    /// Is between `from` and `to`. `from` is at most `to`.
    Range {
        /// The low bound.
        from: f64,
        /// The high bound.
        to: f64,
    },
    /// Matches the pattern (plain form, 4.24).
    Regex(String),
    /// Is null.
    IsNull,
    /// Is not null.
    IsNotNull,
}

/// The object `legend` (4.26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Legend {
    /// `true`: the panel shows a legend.
    pub show: bool,
    /// The place of the legend.
    pub placement: LegendPlacement,
    /// The legend mode.
    pub mode: LegendMode,
    /// The values of the table mode. 0 to 5 items. In a `piechart`: 0 to 2 items.
    pub values: Vec<LegendValue>,
    /// The column that sorts the table mode. `None`: no sort.
    pub sort_by: Option<LegendSortBy>,
    /// `true`: the sort goes from the largest value to the smallest. Default `false`.
    pub sort_desc: bool,
}

/// The object `tooltip` (4.26).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tooltip {
    /// The tooltip mode.
    pub mode: TooltipMode,
    /// The order of the values of a `multi` tooltip. Default `None`.
    pub sort: TooltipSort,
    /// `true`: a `multi` tooltip does not show the values that equal 0. Default `false`.
    pub hide_zeros: bool,
}

/// The object `reduce` (4.26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reduce {
    /// The calculation.
    pub calc: ReduceCalc,
    /// `None`: each number field. Else a pattern of the Grafana form.
    pub fields: Option<String>,
    /// `true`: one item for each row.
    pub values: bool,
}

/// The options of a panel, by type (4.27 to 4.33).
#[derive(Debug, Clone, PartialEq)]
pub enum PanelOptions {
    /// Options of `table`.
    Table(TableOptions),
    /// Options of `timeseries`.
    Timeseries(TimeseriesOptions),
    /// Options of `stat`.
    Stat(StatOptions),
    /// Options of `barchart`.
    Barchart(BarchartOptions),
    /// Options of `piechart`.
    Piechart(PiechartOptions),
    /// Options of `xychart`.
    Xychart(XychartOptions),
    /// Options of `bargauge`.
    Bargauge(BargaugeOptions),
    /// Options of `text`.
    Text(TextOptions),
    /// Options of `state_timeline`.
    StateTimeline(StateTimelineOptions),
    /// Options of `status_history`.
    StatusHistory(StatusHistoryOptions),
    /// Options of `heatmap`.
    Heatmap(HeatmapOptions),
    /// Options of `histogram`.
    Histogram(HistogramOptions),
    /// Options of `logs`.
    Logs(LogsOptions),
    /// Options of `gauge`.
    Gauge(GaugeOptions),
    /// Options of `geomap` (4.33g).
    Geomap(super::geomap::GeomapOptions),
}

impl PanelOptions {
    /// Gives the panel type that these options belong to.
    pub fn panel_type(&self) -> PanelType {
        match self {
            PanelOptions::Table(_) => PanelType::Table,
            PanelOptions::Timeseries(_) => PanelType::Timeseries,
            PanelOptions::Stat(_) => PanelType::Stat,
            PanelOptions::Barchart(_) => PanelType::Barchart,
            PanelOptions::Piechart(_) => PanelType::Piechart,
            PanelOptions::Xychart(_) => PanelType::Xychart,
            PanelOptions::Bargauge(_) => PanelType::Bargauge,
            PanelOptions::Text(_) => PanelType::Text,
            PanelOptions::StateTimeline(_) => PanelType::StateTimeline,
            PanelOptions::StatusHistory(_) => PanelType::StatusHistory,
            PanelOptions::Heatmap(_) => PanelType::Heatmap,
            PanelOptions::Histogram(_) => PanelType::Histogram,
            PanelOptions::Logs(_) => PanelType::Logs,
            PanelOptions::Gauge(_) => PanelType::Gauge,
            PanelOptions::Geomap(_) => PanelType::Geomap,
        }
    }
}

/// A sort key of a table (4.27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortKey {
    /// A display name: 1 to 1024 bytes.
    pub field: String,
    /// `true`: from the largest value to the smallest.
    pub desc: bool,
}

/// Options of `table` (4.27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOptions {
    /// `true`: the table shows its header row.
    pub show_header: bool,
    /// The first sort: 0 to 8 keys.
    pub sort: Vec<SortKey>,
    /// `true`: a long cell text wraps to more lines. Default `false`.
    pub wrap: bool,
}

/// Options of `timeseries` (4.28).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeseriesOptions {
    /// The draw style.
    pub draw: TimeseriesDraw,
    /// The interpolation.
    pub interpolation: Interpolation,
    /// 0 to 10 CSS pixels.
    pub line_width: u8,
    /// 0 to 100 percent.
    pub fill_opacity: u8,
    /// When to show the points.
    pub show_points: ShowPoints,
    /// 1 to 40 CSS pixels.
    pub point_size: u8,
    /// `true`: the line joins the values on both sides of a null.
    pub span_nulls: bool,
    /// The `stacking` mode.
    pub stacking: Stacking,
    /// The scale of the y axis.
    pub y_scale: YScale,
    /// The legend.
    pub legend: Legend,
    /// The tooltip.
    pub tooltip: Tooltip,
    /// How the thresholds show in the chart. Default `Off`.
    pub thresholds_style: ThresholdsStyle,
    /// The gradient of the fill and of the line. Default `None` (wave H, P10).
    pub gradient_mode: GradientMode,
    /// The base of a log scale: 2 or 10. Default 10 (P14).
    pub log_base: u8,
    /// The null threshold in ms: a run of nulls between two values less than this apart joins. `None`: no threshold (P14).
    pub span_nulls_ms: Option<u64>,
    /// The place of a bar against its time. Default `Center` (P14).
    pub bar_alignment: BarAlignment,
}

/// The default of the optional option `log_base` of a `timeseries` (4.28): the base of `y_scale` = `log` before wave H.
pub const LOG_BASE: u8 = 10;
/// The largest value of the optional option `span_nulls_ms` (4.28): 10^12 ms, about 31.7 years.
pub const MAX_SPAN_NULLS_MS: i64 = 1_000_000_000_000;

/// Options of `stat` (4.29).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatOptions {
    /// The reduction.
    pub reduce: Reduce,
    /// The colour mode.
    pub color_mode: StatColorMode,
    /// The graph mode.
    pub graph_mode: GraphMode,
    /// The text mode.
    pub text_mode: TextMode,
    /// The orientation.
    pub orientation: StatOrientation,
    /// The alignment of the items. Default `Auto`.
    pub justify: StatJustify,
    /// `true`: each reduced item shows its percent change (wave H, item P11). Default `false`.
    pub percent_change: bool,
    /// The colour of the percent change. Default `Standard`.
    pub percent_change_color: PercentChangeColor,
    /// The font size of the name, 1 to 200 CSS pixels. `None`: the page fits the size.
    pub title_size: Option<u8>,
    /// The font size of the value, 1 to 200 CSS pixels. `None`: the page fits the size.
    pub value_size: Option<u8>,
}

/// The default of the optional option `fill_opacity` of a `barchart` (4.30): the Grafana default.
pub const BAR_FILL_OPACITY: u8 = 80;
/// The default of the optional option `line_width` of a `barchart` (4.30): the Grafana default.
pub const BAR_LINE_WIDTH: u8 = 1;

/// Options of `barchart` (4.30).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarchartOptions {
    /// The orientation.
    pub orientation: BarchartOrientation,
    /// The category field (1 to 1024 bytes), or `None`.
    pub x_field: Option<String>,
    /// The `stacking` mode.
    pub stacking: Stacking,
    /// When to show the values.
    pub show_values: ShowValues,
    /// 10 to 100 percent.
    pub bar_width: u8,
    /// 10 to 100 percent.
    pub group_width: u8,
    /// -90 to 90 degrees.
    pub x_tick_rotation: i8,
    /// The legend.
    pub legend: Legend,
    /// The tooltip.
    pub tooltip: Tooltip,
    /// How the thresholds show in the chart. Default `Off`.
    pub thresholds_style: ThresholdsStyle,
    /// The opacity of the fill of a bar: 0 to 100 percent. Default 80.
    pub fill_opacity: u8,
    /// The width of the line around a bar: 0 to 10 CSS pixels. Default 1.
    pub line_width: u8,
    /// The gradient of the fill and of the line of the bars. Default `None` (wave H, P10).
    pub gradient_mode: GradientMode,
}

/// Options of `piechart` (4.31).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiechartOptions {
    /// `pie` or `donut`.
    pub pie_type: PieType,
    /// The reduction.
    pub reduce: Reduce,
    /// The labels: 0 to 3 unique items.
    pub labels: Vec<PieLabel>,
    /// The legend, with the values of a `piechart`.
    pub legend: Legend,
    /// The tooltip.
    pub tooltip: Tooltip,
}

/// Options of `xychart` (4.32).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XychartOptions {
    /// The x field (1 to 1024 bytes), or `None`.
    pub x_field: Option<String>,
    /// The y fields: 0 to 32 display names.
    pub y_fields: Vec<String>,
    /// The draw style.
    pub draw: XyDraw,
    /// 1 to 40 CSS pixels.
    pub point_size: u8,
    /// 0 to 10 CSS pixels.
    pub line_width: u8,
    /// The legend.
    pub legend: Legend,
    /// The tooltip.
    pub tooltip: Tooltip,
}

/// Options of `bargauge` (4.33).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BargaugeOptions {
    /// The reduction.
    pub reduce: Reduce,
    /// The orientation.
    pub orientation: Orientation,
    /// The display mode.
    pub display_mode: BarGaugeMode,
    /// `true`: the empty part of each bar shows.
    pub show_unfilled: bool,
}

/// The default of the optional options `min_width` and `min_height` of a `gauge` (4.33e): 75 CSS pixels, the Grafana default.
pub const GAUGE_MIN_SIZE: u16 = 75;
/// The largest value of the optional options `min_width` and `min_height` of a `gauge` (4.33e): the end of the slider of Grafana.
pub const GAUGE_MAX_MIN_SIZE: u16 = 600;
/// The largest value of the optional options `title_size` and `value_size` of a `gauge` (4.33e), in CSS pixels.
pub const GAUGE_MAX_TEXT_SIZE: u8 = 200;

/// Options of `gauge` (4.33e): the classic gauge of Grafana 12.4.2. Each member but `reduce` is optional, and an absent member has
/// the Grafana default.
#[derive(Debug, Clone, PartialEq)]
pub struct GaugeOptions {
    /// The reduction.
    pub reduce: Reduce,
    /// The place of the gauges. Default `Auto`: a grid.
    pub orientation: StatOrientation,
    /// `true`: the values of the thresholds, the ends and the neutral value show around the arc. Default `false`.
    pub show_threshold_labels: bool,
    /// `true`: the band of the threshold colours shows outside the arc. Default `true`.
    pub show_threshold_markers: bool,
    /// The value where the filled arc starts, or `None` (the start of the scale). Default `None`.
    pub neutral: Option<f64>,
    /// The size of the names (1 to 200 CSS pixels), or `None` for the automatic size. Default `None`.
    pub title_size: Option<u8>,
    /// The size of the values (1 to 200 CSS pixels), or `None` for the automatic size. Default `None`.
    pub value_size: Option<u8>,
    /// `Manual` reads `min_width` and `min_height`. Default `Auto`.
    pub sizing: GaugeSizing,
    /// The least width of a gauge in a row (0 to 600 CSS pixels), read with `Manual` and `vertical`. Default 75.
    pub min_width: u16,
    /// The least height of a gauge in a column (0 to 600 CSS pixels), read with `Manual` and `horizontal`. Default 75.
    pub min_height: u16,
}

/// Options of `text` (4.33a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextOptions {
    /// The mode. `markdown` is the only value.
    pub mode: TextPanelMode,
    /// The lines of the Markdown text: 0 to `MAX_TEXT_LINES` lines, each without U+000A and U+000D.
    pub content: Vec<String>,
}

/// Options of `state_timeline` (4.33b). The legend has the mode `list` and no values. The tooltip has no sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTimelineOptions {
    /// `true`: consecutive samples with the same state make one segment.
    pub merge_values: bool,
    /// When to show the text in a segment.
    pub show_values: ShowValues,
    /// The place of the text in a segment.
    pub align_values: AlignValues,
    /// 10 to 100 percent of the plot height.
    pub row_height: u8,
    /// 0 to 10 CSS pixels.
    pub line_width: u8,
    /// 0 to 100 percent.
    pub fill_opacity: u8,
    /// `true`: the segment goes on over a sample with no value.
    pub span_nulls: bool,
    /// `true`: the name of each row shows at the left of the plot.
    pub show_labels: bool,
    /// The legend.
    pub legend: Legend,
    /// The tooltip.
    pub tooltip: Tooltip,
}

/// Options of `status_history` (4.33b). The legend has the mode `list` and no values. The tooltip has no sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusHistoryOptions {
    /// When to show the text in a cell.
    pub show_values: ShowValues,
    /// 10 to 100 percent of the plot height.
    pub row_height: u8,
    /// 10 to 100 percent of the step between two samples.
    pub col_width: u8,
    /// 0 to 10 CSS pixels.
    pub line_width: u8,
    /// 0 to 100 percent.
    pub fill_opacity: u8,
    /// `true`: the name of each row shows at the left of the plot.
    pub show_labels: bool,
    /// The legend.
    pub legend: Legend,
    /// The tooltip.
    pub tooltip: Tooltip,
}

/// The object `x_buckets` of a `heatmap` (4.33c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatXBuckets {
    /// `count`: about `value` buckets in the range. `size`: each bucket has the length `value`.
    pub mode: BucketMode,
    /// With `count`: 1 to 1,000, or `None` for 20 buckets. With `size`: milliseconds, 1 to 31,536,000,000.
    pub value: Option<i64>,
}

/// The object `y_buckets` of a `heatmap` (4.33c).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeatYBuckets {
    /// `count`: about `value` buckets. `size`: each bucket has the width `value`. A log scale needs `size`.
    pub mode: BucketMode,
    /// The count, the width or the split of a log scale (4.33c). `None`: the default of the scale.
    pub value: Option<f64>,
    /// The scale of the value axis and of the buckets.
    pub scale: HeatScale,
}

/// The object `color` of a `heatmap` (4.33c).
#[derive(Debug, Clone, PartialEq)]
pub struct HeatColor {
    /// `scheme`: a named scheme. `opacity`: one colour with a changing opacity.
    pub mode: HeatColorMode,
    /// The scheme of the mode `scheme`.
    pub scheme: HeatScheme,
    /// A colour of 4.19: the colour of the mode `opacity`.
    pub fill: String,
    /// The rule of the opacity in the mode `opacity`.
    pub scale: HeatColorScale,
    /// 0.1 to 2, with one decimal at most.
    pub exponent: f64,
    /// 2 to 128: the number of colours of the scale.
    pub steps: u8,
    /// `true`: the scale goes from the last colour to the first colour.
    pub reverse: bool,
    /// The count that has the first colour. `None`: the lowest count that shows.
    pub min: Option<f64>,
    /// The count that has the last colour. `None`: the highest count that shows.
    pub max: Option<f64>,
}

/// Options of `heatmap` (4.33c). The tooltip has no sort and no hide-zeros.
#[derive(Debug, Clone, PartialEq)]
pub struct HeatmapOptions {
    /// `false`: the frame is already bucketed. `true`: the page buckets the values.
    pub calculate: bool,
    /// The time buckets of `calculate` = `true`.
    pub x_buckets: HeatXBuckets,
    /// The value buckets of `calculate` = `true`.
    pub y_buckets: HeatYBuckets,
    /// The place of the value axis.
    pub y_axis: HeatYAxis,
    /// `true`: the small values are at the top.
    pub y_reverse: bool,
    /// The lowest value of the axis. `None`: the lowest edge of the cells.
    pub y_min: Option<f64>,
    /// The highest value of the axis. `None`: the highest edge of the cells.
    pub y_max: Option<f64>,
    /// The colour scale.
    pub color: HeatColor,
    /// 0 to 25 CSS pixels.
    pub cell_gap: u8,
    /// A cell with a count at or below this number is not drawn. `None`: no such rule.
    pub hide_le: Option<f64>,
    /// A cell with a count at or above this number is not drawn. `None`: no such rule.
    pub hide_ge: Option<f64>,
    /// `true`: the colour bar shows under the plot.
    pub show_legend: bool,
    /// The tooltip.
    pub tooltip: Tooltip,
}

/// Options of `histogram` (4.33d).
#[derive(Debug, Clone, PartialEq)]
pub struct HistogramOptions {
    /// 1 to 1,000: the number of buckets when `bucket_size` is `None`.
    pub bucket_count: u16,
    /// A number above 0: the width of a bucket. `None`: the page chooses the width.
    pub bucket_size: Option<f64>,
    /// Any finite number: the edges are this offset plus a whole number of widths.
    pub bucket_offset: f64,
    /// `true`: one bar series for all the fields.
    pub combine: bool,
    /// The stacking of the bar series.
    pub stacking: Stacking,
    /// 0 to 10 CSS pixels.
    pub line_width: u8,
    /// 0 to 100 percent.
    pub fill_opacity: u8,
    /// The legend.
    pub legend: Legend,
    /// The tooltip.
    pub tooltip: Tooltip,
}

/// Options of `logs` (4.33f). Each member is optional in the document: an absent member has the default
/// that the comment gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogsOptions {
    /// `true`: each line starts with its time. Default `false`.
    pub show_time: bool,
    /// `true`: a long line wraps to more rows of text. Default `false`.
    pub wrap: bool,
    /// `true`: a click on a line opens its details row. Default `true`.
    pub details: bool,
    /// The order of the lines. Default `newest_first`.
    pub order: LogsOrder,
}

impl TextOptions {
    /// Gives the text: the lines joined with one U+000A between two lines (4.33a, rule 2).
    pub fn text(&self) -> String {
        self.content.join("\n")
    }
}

impl Document {
    /// Gives the uid.
    pub fn uid(&self) -> &Uid {
        &self.uid
    }

    /// Gives the title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Gives the sections in document order.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// Gives the variables in document order.
    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    /// Gives each panel of the document with the index of its section, in document order.
    pub fn panels(&self) -> impl Iterator<Item = (usize, &Panel)> {
        self.sections.iter().enumerate().flat_map(|(i, s)| s.panels.iter().map(move |p| (i, p)))
    }

    /// Finds a panel by its id.
    pub fn panel(&self, id: u32) -> Option<&Panel> {
        self.panels().map(|(_, p)| p).find(|p| p.id == id)
    }

    /// Finds a variable by its name.
    pub fn variable(&self, name: &str) -> Option<&Variable> {
        self.variables.iter().find(|v| v.name == name)
    }

    /// Gives the annotations in document order (4.5b).
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Lists every SQL text of the document with its JSON pointer: first the variables, then the
    /// annotations, then the panels with their own query, in document order (7.18).
    pub fn sql_texts(&self) -> Vec<SqlText<'_>> {
        let mut out = Vec::new();
        for (i, v) in self.variables.iter().enumerate() {
            if let VariableKind::Query { source, sql, .. } = &v.kind {
                out.push(SqlText {
                    pointer: format!("/variables/{i}/sql"),
                    owner: SqlOwner::Variable { name: &v.name },
                    source,
                    lines: sql,
                });
            }
        }
        for (i, a) in self.annotations.iter().enumerate() {
            out.push(SqlText {
                pointer: format!("/annotations/{i}/sql"),
                owner: SqlOwner::Annotation { index: i },
                source: &a.source,
                lines: &a.sql,
            });
        }
        for (si, section) in self.sections.iter().enumerate() {
            for (pi, p) in section.panels.iter().enumerate() {
                if let PanelData::Query(q) = &p.data {
                    out.push(SqlText {
                        pointer: format!("/sections/{si}/panels/{pi}/data/query/sql"),
                        owner: SqlOwner::Panel { id: p.id },
                        source: &q.source,
                        lines: &q.sql,
                    });
                    // The queries of 4.15a: hidden queries too, because a save of SQL is the power to run it.
                    for (qi, e) in q.extra_queries.iter().enumerate() {
                        out.push(SqlText {
                            pointer: format!("/sections/{si}/panels/{pi}/data/extra_queries/{qi}/sql"),
                            owner: SqlOwner::Panel { id: p.id },
                            source: &e.source,
                            lines: &e.sql,
                        });
                    }
                }
            }
        }
        out
    }
}

/// The owner of one SQL text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlOwner<'a> {
    /// A `query` variable.
    Variable {
        /// The name of the variable.
        name: &'a str,
    },
    /// A panel with its own query.
    Panel {
        /// The id of the panel.
        id: u32,
    },
    /// An annotation (4.5b).
    Annotation {
        /// The position of the annotation in the array `annotations`, from 0.
        index: usize,
    },
}

/// One SQL text of a document, with its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlText<'a> {
    /// The JSON pointer of the array of lines, for example `/sections/0/panels/2/data/query/sql`.
    pub pointer: String,
    /// The owner of the text.
    pub owner: SqlOwner<'a>,
    /// The data source of the text.
    pub source: &'a SourceId,
    /// The SQL lines.
    pub lines: &'a [String],
}

impl SqlText<'_> {
    /// Gives the SQL text: the lines joined with one U+000A between two lines (4.16).
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Gives the JSON pointer of one line, for example `/variables/1/sql/5`.
    pub fn line_pointer(&self, line: usize) -> String {
        format!("{}/{line}", self.pointer)
    }
}
