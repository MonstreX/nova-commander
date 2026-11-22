//! The colours of the syntax highlighting's classes (ADR-0024 §3): a role `syntax.*` for each of
//! Colorer's `def:` regions with a colour of its own.

use ratatui::style::Style;

use super::role::Role;
use super::style;

/// A syntax class's colours (Colorer's console colours by default, ADR-0024 §3); none for a class no
/// role stands for.
pub fn syntax(class: &str) -> Option<Style> {
    let role = match class {
        "Number" => Role::SyntaxNumber,
        "NumberSuffix" => Role::SyntaxNumberSuffix,
        "String" => Role::SyntaxString,
        "StringContent" => Role::SyntaxStringContent,
        "StringEdge" => Role::SyntaxStringEdge,
        "Character" => Role::SyntaxCharacter,
        "CharacterContent" => Role::SyntaxCharacterContent,
        "Comment" => Role::SyntaxComment,
        "CommentContent" => Role::SyntaxCommentContent,
        "CommentEdge" => Role::SyntaxCommentEdge,
        "CommentDoc" => Role::SyntaxCommentDoc,
        "CommentDocContent" => Role::SyntaxCommentDocContent,
        "CommentDocEdge" => Role::SyntaxCommentDocEdge,
        "Symbol" => Role::SyntaxSymbol,
        "SymbolStrong" => Role::SyntaxSymbolStrong,
        "Operator" => Role::SyntaxOperator,
        "Prefix" => Role::SyntaxPrefix,
        "PrefixStrong" => Role::SyntaxPrefixStrong,
        "Keyword" => Role::SyntaxKeyword,
        "TypeKeyword" => Role::SyntaxKeywordType,
        "Constant" => Role::SyntaxConstant,
        "BooleanConstant" => Role::SyntaxConstantBoolean,
        "Register" => Role::SyntaxRegister,
        "Var" => Role::SyntaxVar,
        "VarStrong" => Role::SyntaxVarStrong,
        "Identifier" => Role::SyntaxIdentifier,
        "Directive" => Role::SyntaxDirective,
        "DirectiveInternal" => Role::SyntaxDirectiveInternal,
        "Parameter" => Role::SyntaxParameter,
        "ParameterStrong" => Role::SyntaxParameterStrong,
        "ParameterUnknown" => Role::SyntaxParameterUnknown,
        "Tag" => Role::SyntaxTag,
        "OpenTag" => Role::SyntaxTagOpen,
        "CloseTag" => Role::SyntaxTagClose,
        "Label" => Role::SyntaxLabel,
        "LabelStrong" => Role::SyntaxLabelStrong,
        "Insertion" => Role::SyntaxInsertion,
        "Error" => Role::SyntaxError,
        "ErrorText" => Role::SyntaxErrorText,
        "TODO" => Role::SyntaxTodo,
        "Debug" => Role::SyntaxDebug,
        "URI" => Role::SyntaxUri,
        "EMail" => Role::SyntaxEmail,
        "Date" => Role::SyntaxDate,
        "Time" => Role::SyntaxTime,
        "PairStart" => Role::SyntaxPair,
        "PairStrongStart" => Role::SyntaxPairStrong,
        "PairEnd" => Role::SyntaxPair,
        "PairStrongEnd" => Role::SyntaxPairStrong,
        _ => return None,
    };
    Some(style(role))
}
