//! The contexts of a placeholder and the document errors of 7.8.
//!
//! The expansion and the checks at save use the same function, so the two agree.

use super::interval::{GroupIntervalError, GroupIntervalFault, check_stored_group_interval};
use super::lexer::{Token, TokenKind};
use super::placeholders::Format;

/// The lexical context of a placeholder (7.6, 7.8). A comment is not a context: it is not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A run of code tokens.
    Code,
    /// The content of a string.
    String,
    /// The content of a quoted name.
    QuotedName,
    /// The content of a heredoc.
    Heredoc,
    /// The content of an `E` string of the dialect `postgres` (7.21). It has the rules of a string.
    EString,
    /// The content of a prefixed string or of a `U&"..."` name of the dialect `postgres` (7.21).
    Prefixed,
    /// The content of a dollar quote of the dialect `postgres` (7.21).
    DollarQuote,
}

/// A placeholder in a context that 7.8 marks as a document error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextFault {
    /// A placeholder in a heredoc.
    InHeredoc,
    /// The format `singlequote` or `sqlstring` in a string.
    QuoteFormatInString,
    /// A list in a string with the format none or `raw`.
    ListInString,
    /// A variable that is not `custom` in a quoted name.
    NameNeedsCustom,
    /// A format that is valid only inside a string, in code or in a quoted name (D7).
    LiteralFormatOutsideString(Format),
    /// A placeholder in a prefixed string or in a `U&"..."` name (7.21).
    InPrefixed,
    /// A placeholder in a dollar quote (7.21).
    InDollarQuote,
    /// A list in a quoted name.
    ListInName,
    /// The format `singlequote` or `sqlstring` in a quoted name.
    QuoteFormatInName,
}

impl ContextFault {
    /// Gives a short English message about the variable `name`.
    pub fn message(self, name: &str) -> String {
        match self {
            ContextFault::InHeredoc => {
                format!("the variable {name} is in a heredoc: no placeholder is permitted there")
            }
            ContextFault::InPrefixed => {
                format!("the variable {name} is in a prefixed string or a U& name: no placeholder is permitted there")
            }
            ContextFault::InDollarQuote => {
                format!("the variable {name} is in a dollar quote: no placeholder is permitted there")
            }
            ContextFault::QuoteFormatInString => {
                format!("the variable {name} has the format singlequote or sqlstring inside a string")
            }
            ContextFault::ListInString => {
                format!("the variable {name} can be a list inside a string: use the format csv")
            }
            ContextFault::LiteralFormatOutsideString(f) => format!(
                "the variable {name} has the format {}: this format is permitted only inside a string",
                f.name()
            ),
            ContextFault::NameNeedsCustom => {
                format!("the variable {name} is in a quoted name: only a custom variable is permitted there")
            }
            ContextFault::ListInName => format!("the variable {name} can be a list inside a quoted name"),
            ContextFault::QuoteFormatInName => {
                format!("the variable {name} has the format singlequote or sqlstring inside a quoted name")
            }
        }
    }
}

/// Applies the table of 7.8 to one placeholder. The dialect `postgres` uses the same table (7.21), with the
/// places `EString` (the rules of a string), `Prefixed` and `DollarQuote` (always a document error).
///
/// `is_custom` tells if the variable is a closed `custom` variable: an open one has the rows of a `query`
/// variable (7.8). `is_list` tells if the selection is a
/// list. At run time it is the real selection. At save it is [`super::values::VariableDef::may_be_list`].
pub fn context_fault(place: Place, is_custom: bool, is_list: bool, format: Option<Format>) -> Option<ContextFault> {
    let quote_format = matches!(format, Some(Format::SingleQuote | Format::SqlString));
    let literal_only = format.filter(|f| f.literal_only());
    match place {
        Place::Code | Place::QuotedName if literal_only.is_some() => {
            literal_only.map(ContextFault::LiteralFormatOutsideString)
        }
        Place::Code => None,
        Place::Heredoc => Some(ContextFault::InHeredoc),
        Place::Prefixed => Some(ContextFault::InPrefixed),
        Place::DollarQuote => Some(ContextFault::InDollarQuote),
        Place::String | Place::EString => {
            if quote_format {
                Some(ContextFault::QuoteFormatInString)
            } else if is_list && matches!(format, None | Some(Format::Raw)) {
                Some(ContextFault::ListInString)
            } else {
                None
            }
        }
        Place::QuotedName => {
            if !is_custom {
                Some(ContextFault::NameNeedsCustom)
            } else if is_list {
                Some(ContextFault::ListInName)
            } else if quote_format {
                Some(ContextFault::QuoteFormatInName)
            } else {
                None
            }
        }
    }
}

/// Checks the shape of the stored second argument of a group macro (7.23), on the tokens `lo..hi` after the
/// removal of the blanks at both ends. The argument MUST be one string token, or the text `$__interval`
/// or `${__interval}`. The grammar of a string is checked when it has no placeholder
/// ([`check_stored_group_interval`]). The expansion and the checks at save use the same function.
pub(crate) fn interval_argument_shape(
    src: &str,
    tokens: &[Token],
    lo: usize,
    hi: usize,
) -> Result<(), GroupIntervalError> {
    let (Some(first), Some(last)) = (tokens.get(lo), tokens.get(hi.wrapping_sub(1))) else {
        return Err(GroupIntervalError {
            fault: GroupIntervalFault::Shape,
            message: "the interval must be one string like '5m', or $__interval".to_owned(),
        });
    };
    let text = src.get(first.start..last.end).unwrap_or("");
    if text.starts_with('\'') && !(hi - lo == 1 && first.kind == TokenKind::String && first.closed) {
        return Err(GroupIntervalError {
            fault: GroupIntervalFault::Shape,
            message: "the interval must be one string like '5m', or $__interval".to_owned(),
        });
    }
    check_stored_group_interval(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_of_7_8() {
        use Format::*;
        use Place::*;
        // String.
        for f in [None, Some(Raw), Some(Csv)] {
            assert_eq!(context_fault(String, false, false, f), Option::None);
        }
        assert_eq!(context_fault(String, false, true, Some(Csv)), Option::None);
        assert_eq!(context_fault(String, false, true, Option::None), Some(ContextFault::ListInString));
        assert_eq!(context_fault(String, true, true, Some(Raw)), Some(ContextFault::ListInString));
        assert_eq!(context_fault(String, false, false, Some(SingleQuote)), Some(ContextFault::QuoteFormatInString));
        assert_eq!(context_fault(String, true, false, Some(SqlString)), Some(ContextFault::QuoteFormatInString));
        // Quoted name.
        assert_eq!(context_fault(QuotedName, true, false, Option::None), Option::None);
        assert_eq!(context_fault(QuotedName, true, false, Some(Csv)), Option::None);
        assert_eq!(context_fault(QuotedName, false, false, Option::None), Some(ContextFault::NameNeedsCustom));
        assert_eq!(context_fault(QuotedName, true, true, Some(Csv)), Some(ContextFault::ListInName));
        assert_eq!(context_fault(QuotedName, true, false, Some(SingleQuote)), Some(ContextFault::QuoteFormatInName));
        // Heredoc and code.
        assert_eq!(context_fault(Heredoc, true, false, Option::None), Some(ContextFault::InHeredoc));
        assert_eq!(context_fault(Code, false, true, Some(SqlString)), Option::None);
        // The formats that are valid only inside a string (D7).
        for f in crate::sql::placeholders::LITERAL_ONLY_FORMATS {
            assert_eq!(context_fault(String, false, false, Some(f)), Option::None, "{f:?}");
            assert_eq!(context_fault(String, false, true, Some(f)), Option::None, "{f:?}");
            assert_eq!(context_fault(String, true, true, Some(f)), Option::None, "{f:?}");
            let fault = Some(ContextFault::LiteralFormatOutsideString(f));
            assert_eq!(context_fault(Code, false, false, Some(f)), fault, "{f:?}");
            assert_eq!(context_fault(Code, true, true, Some(f)), fault, "{f:?}");
            assert_eq!(context_fault(QuotedName, true, false, Some(f)), fault, "{f:?}");
            assert_eq!(context_fault(Heredoc, true, false, Some(f)), Some(ContextFault::InHeredoc), "{f:?}");
        }
    }
}
