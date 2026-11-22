//! The colour roles a theme gives (ADR-0008): dotted names in theme files, an enum in code. A role a
//! theme does not give falls back to its parent: `menu.selected.hotkey` -> `menu.selected` -> `menu`.

macro_rules! roles {
    ($($role:ident = $name:literal, FAR $far:literal;)*) => {
        /// What is coloured. The FAR palette entry it stands for is in the comment of each.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Role {
            $(#[doc = $far] $role,)*
        }

        impl Role {
            pub const ALL: &[Role] = &[$(Role::$role,)*];

            /// The name in theme files.
            pub fn name(self) -> &'static str {
                match self {
                    $(Role::$role => $name,)*
                }
            }
        }
    };
}

roles! {
    Panel = "panel", FAR "Panel.Text";
    PanelFrame = "panel.frame", FAR "Panel.Box";
    PanelTitle = "panel.title", FAR "Panel.Title";
    PanelTitleFocused = "panel.title.focused", FAR "Panel.Title.Selected";
    PanelHeader = "panel.header", FAR "Panel.Title.Column";
    PanelDrag = "panel.drag", FAR "Panel.DragText";
    PanelSelected = "panel.selected", FAR "Panel.Text.Selected";
    PanelCursor = "panel.cursor", FAR "Panel.Cursor";
    PanelCursorSelected = "panel.cursor.selected", FAR "Panel.Cursor.Selected";
    PanelTotal = "panel.total", FAR "Panel.Info.Total";
    PanelScrollbar = "panel.scrollbar", FAR "Panel.Scrollbar";
    PanelSelectedInfo = "panel.selectedInfo", FAR "Panel.Info.Selected";
    PanelInfo = "panel.info", FAR "Panel.Info.Text";
    PanelHighlight = "panel.highlight", FAR "Panel.Text.Highlight";
    PanelError = "panel.error", FAR "(none: an error shown inside a panel)";
    Clock = "clock", FAR "Clock";
    MenuBar = "menuBar", FAR "HMenu.Text";
    MenuBarHotkey = "menuBar.hotkey", FAR "HMenu.Highlight";
    Menu = "menu", FAR "Menu.Text";
    MenuFrame = "menu.frame", FAR "Menu.Box";
    MenuHotkey = "menu.hotkey", FAR "Menu.Highlight";
    MenuSelected = "menu.selected", FAR "Menu.Text.Selected, HMenu.Text.Selected";
    MenuSelectedHotkey = "menu.selected.hotkey", FAR "Menu.Highlight.Selected";
    MenuDisabled = "menu.disabled", FAR "Menu.GrayText";
    MenuSelectedDisabled = "menu.selected.disabled", FAR "Menu.Selected.GrayText";
    MenuShadow = "shadow", FAR "(none: FAR's DropShadow, dark grey on black)";
    KeybarKey = "keybar.key", FAR "Keybar.Num";
    KeybarLabel = "keybar.label", FAR "Keybar.Text";
    CommandLine = "commandLine", FAR "CommandLine";
    CommandLineSelected = "commandLine.selected", FAR "CommandLine.Selected";
    Dialog = "dialog", FAR "Dialog.Text, Dialog.Box";
    DialogTitle = "dialog.title", FAR "Dialog.Box.Title";
    DialogHotkey = "dialog.hotkey", FAR "Dialog.Text.Highlight";
    DialogDisabled = "dialog.disabled", FAR "Dialog.Disabled";
    DialogDrag = "dialog.drag", FAR "(none: FAR's yellow on light red marks in the corners of a dialog moved by keys)";
    DialogButton = "dialog.button", FAR "Dialog.Button";
    DialogButtonHotkey = "dialog.button.hotkey", FAR "Dialog.Button.Highlight";
    DialogButtonFocused = "dialog.button.focused", FAR "Dialog.Button.Selected";
    DialogButtonFocusedHotkey = "dialog.button.focused.hotkey", FAR "Dialog.Button.Highlight.Selected";
    DialogButtonShadow = "dialog.button.shadow", FAR "(none: a Turbo Vision button's shadow)";
    DialogEdit = "dialog.edit", FAR "Dialog.Edit";
    DialogEditSelected = "dialog.edit.selected", FAR "Dialog.Edit.Selected";
    DialogEditDisabled = "dialog.edit.disabled", FAR "Dialog.Edit.Disabled";
    DialogList = "dialog.list", FAR "Dialog.Combo.Text, Dialog.Combo.Box";
    DialogListHotkey = "dialog.list.hotkey", FAR "Dialog.Combo.Highlight";
    DialogListSelected = "dialog.list.selected", FAR "Dialog.Combo.Text.Selected";
    DialogListSelectedHotkey = "dialog.list.selected.hotkey", FAR "Dialog.Combo.Highlight.Selected";
    DialogListGray = "dialog.list.gray", FAR "Dialog.Combo.GrayText";
    DialogListSelectedGray = "dialog.list.selected.gray", FAR "Dialog.Combo.GrayText.Selected";
    DialogListBox = "dialog.listbox", FAR "Dialog.List.Text";
    DialogListBoxSelected = "dialog.listbox.selected", FAR "Dialog.List.Text.Selected";
    Warning = "warning", FAR "WarnDialog.Text";
    WarningHotkey = "warning.hotkey", FAR "WarnDialog.Text.Highlight";
    WarningButton = "warning.button", FAR "WarnDialog.Button";
    WarningButtonHotkey = "warning.button.hotkey", FAR "WarnDialog.Button.Highlight";
    WarningButtonFocused = "warning.button.focused", FAR "WarnDialog.Button.Selected";
    WarningButtonFocusedHotkey = "warning.button.focused.hotkey", FAR "WarnDialog.Button.Highlight.Selected";
    WarningButtonShadow = "warning.button.shadow", FAR "(none: a Turbo Vision button's shadow)";
    Text = "text", FAR "Viewer.Text, Editor.Text";
    TextStatus = "text.status", FAR "Viewer.Status, Editor.Status";
    TextSelected = "text.selected", FAR "Viewer.Text.Selected";
    TextArrows = "text.arrows", FAR "Viewer.Arrows";
    TextScrollbar = "text.scrollbar", FAR "Viewer.Scrollbar";
    TextNumbers = "text.numbers", FAR "Editor.LineNumbers";
    TextPair = "text.pair", FAR "(ours: the code mode's bracket and its pair)";
    TextOccurrence = "text.occurrence", FAR "(ours: the code mode's word under the cursor, each)";
    TextRuler = "text.ruler", FAR "(ours: the code mode's ruler column)";
    Syntax = "syntax", FAR "(Colorer's highlighted text)";
    SyntaxNumber = "syntax.number", FAR "Colorer def:Number";
    SyntaxNumberSuffix = "syntax.number.suffix", FAR "Colorer def:NumberSuffix";
    SyntaxString = "syntax.string", FAR "Colorer def:String";
    SyntaxStringContent = "syntax.string.content", FAR "Colorer def:StringContent";
    SyntaxStringEdge = "syntax.string.edge", FAR "Colorer def:StringEdge";
    SyntaxCharacter = "syntax.string.character", FAR "Colorer def:Character";
    SyntaxCharacterContent = "syntax.string.character.content", FAR "Colorer def:CharacterContent";
    SyntaxComment = "syntax.comment", FAR "Colorer def:Comment";
    SyntaxCommentContent = "syntax.comment.content", FAR "Colorer def:CommentContent";
    SyntaxCommentEdge = "syntax.comment.edge", FAR "Colorer def:CommentEdge";
    SyntaxCommentDoc = "syntax.comment.doc", FAR "Colorer def:CommentDoc";
    SyntaxCommentDocContent = "syntax.comment.doc.content", FAR "Colorer def:CommentDocContent";
    SyntaxCommentDocEdge = "syntax.comment.doc.edge", FAR "Colorer def:CommentDocEdge";
    SyntaxSymbol = "syntax.symbol", FAR "Colorer def:Symbol";
    SyntaxSymbolStrong = "syntax.symbol.strong", FAR "Colorer def:SymbolStrong";
    SyntaxOperator = "syntax.symbol.operator", FAR "Colorer def:Operator";
    SyntaxPrefix = "syntax.prefix", FAR "Colorer def:Prefix";
    SyntaxPrefixStrong = "syntax.prefix.strong", FAR "Colorer def:PrefixStrong";
    SyntaxKeyword = "syntax.keyword", FAR "Colorer def:Keyword";
    SyntaxKeywordType = "syntax.keyword.type", FAR "Colorer def:TypeKeyword";
    SyntaxConstant = "syntax.keyword.constant", FAR "Colorer def:Constant";
    SyntaxConstantBoolean = "syntax.keyword.constant.boolean", FAR "Colorer def:BooleanConstant";
    SyntaxRegister = "syntax.register", FAR "Colorer def:Register";
    SyntaxVar = "syntax.var", FAR "Colorer def:Var";
    SyntaxVarStrong = "syntax.var.strong", FAR "Colorer def:VarStrong";
    SyntaxIdentifier = "syntax.identifier", FAR "Colorer def:Identifier";
    SyntaxDirective = "syntax.directive", FAR "Colorer def:Directive";
    SyntaxDirectiveInternal = "syntax.directive.internal", FAR "Colorer def:DirectiveInternal";
    SyntaxParameter = "syntax.parameter", FAR "Colorer def:Parameter";
    SyntaxParameterStrong = "syntax.parameter.strong", FAR "Colorer def:ParameterStrong";
    SyntaxParameterUnknown = "syntax.parameter.strong.unknown", FAR "Colorer def:ParameterUnknown";
    SyntaxTag = "syntax.tag", FAR "Colorer def:Tag";
    SyntaxTagOpen = "syntax.tag.open", FAR "Colorer def:OpenTag";
    SyntaxTagClose = "syntax.tag.close", FAR "Colorer def:CloseTag";
    SyntaxLabel = "syntax.label", FAR "Colorer def:Label";
    SyntaxLabelStrong = "syntax.label.strong", FAR "Colorer def:LabelStrong";
    SyntaxInsertion = "syntax.insertion", FAR "Colorer def:Insertion";
    SyntaxError = "syntax.error", FAR "Colorer def:Error";
    SyntaxErrorText = "syntax.error.text", FAR "Colorer def:ErrorText";
    SyntaxTodo = "syntax.error.todo", FAR "Colorer def:TODO";
    SyntaxDebug = "syntax.error.debug", FAR "Colorer def:Debug";
    SyntaxUri = "syntax.uri", FAR "Colorer def:URI";
    SyntaxEmail = "syntax.uri.email", FAR "Colorer def:EMail";
    SyntaxDate = "syntax.date", FAR "Colorer def:Date";
    SyntaxTime = "syntax.time", FAR "Colorer def:Time";
    SyntaxPair = "syntax.pair", FAR "Colorer def:PairStart";
    SyntaxPairStrong = "syntax.pair.strong", FAR "Colorer def:PairStrongStart";
}

impl Role {
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|role| role.name() == name)
    }

    /// The role it falls back to when a theme does not give it: the name without its last part.
    pub fn parent(self) -> Option<Self> {
        let (parent, _) = self.name().rsplit_once('.')?;
        Self::from_name(parent)
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|&role| role == self)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_parents_are_roles() {
        for (index, role) in Role::ALL.iter().enumerate() {
            assert_eq!(role.index(), index);
            assert_eq!(Role::from_name(role.name()), Some(*role));
        }
        assert_eq!(Role::MenuSelectedHotkey.parent(), Some(Role::MenuSelected));
        assert_eq!(Role::MenuSelected.parent(), Some(Role::Menu));
        assert_eq!(Role::Menu.parent(), None);
        assert_eq!(Role::DialogEditSelected.parent(), Some(Role::DialogEdit));
    }
}
