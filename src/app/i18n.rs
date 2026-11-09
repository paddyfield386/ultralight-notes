//! Application-localized UI strings. English is the default language.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Language {
    #[default]
    English,
    Japanese,
}

impl Language {
    pub(super) const fn label(self) -> &'static str {
        match self { Self::English => "English", Self::Japanese => "日本語" }
    }

    pub(super) const fn text(self, english: &'static str, japanese: &'static str) -> &'static str {
        match self { Self::English => english, Self::Japanese => japanese }
    }

    /// Labels shared by the compact and full ribbon buttons.
    pub(super) fn ribbon_text<'a>(self, english: &'a str) -> &'a str {
        if matches!(self, Self::English) { return english; }
        match english {
            "New" => "新規", "Open" => "開く", "Save" => "保存", "Undo" => "元に戻す", "Redo" => "やり直す",
            "Add Child" => "子を追加", "Add Sibling" => "兄弟を追加", "Delete" => "削除",
            "Arrange Branch" => "枝を整理", "Arrange All" => "全体を整理", "Fit All" => "全体を表示",
            "Center on Selection" => "選択へ移動", "Collapse" => "折りたたむ", "Expand" => "展開",
            "Structured JSON" => "構造JSON", "Sticky Note" => "付箋", "Frame" => "フレーム",
            "Text" => "テキスト", "Add Map" => "マップ追加", "Link" => "リンク", "Rectangle" => "矩形",
            "Rounded" => "角丸", "Ellipse" => "楕円", "Decision" => "判断", "Line" => "直線",
            "Connector" => "接続", "Orthogonal Connector" => "直角接続", _ => english,
        }
    }
}
