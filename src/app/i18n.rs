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

    /// Common ribbon and menu labels. Keeping this mapping in one place also
    /// prevents compact-ribbon tooltips from silently falling back to English.
    pub(super) fn ribbon_text<'a>(self, english: &'a str) -> &'a str {
        if matches!(self, Self::English) { return english; }
        match english {
            "Text Box" => "テキストボックス",
            "Home" => "ホーム", "Insert" => "挿入",
            "File" => "ファイル", "Edit" => "編集", "Node" => "ノード",
            "Arrange" => "配置", "View" => "表示", "Branch" => "ブランチ",
            "For AI" => "AI 用", "Color" => "色", "Canvas" => "キャンバス",
            "Shapes" => "図形", "Lines" => "線", "Text" => "テキスト", "Link" => "リンク",
            "New" => "新規", "Open" => "開く", "Save" => "保存", "Save As…" => "名前を付けて保存…",
            "Undo" => "元に戻す", "Redo" => "やり直す",
            "Add Child" => "子を追加", "Add Sibling" => "同階層を追加", "Delete" => "削除",
            "Arrange Branch" => "ブランチを整列", "Arrange All" => "すべて整列",
            "Fit All" => "全体を表示", "Center on Selection" => "選択範囲を中央に表示",
            "Collapse" => "折りたたむ", "Expand" => "展開", "Structured JSON" => "構造化 JSON",
            "Copy Structured JSON" => "構造化 JSON をコピー", "Sticky Note" => "付箋", "Frame" => "フレーム",
            "Add Map" => "マップを追加", "Rectangle" => "長方形", "Rounded" => "角丸長方形",
            "Ellipse" => "楕円", "Decision" => "ひし形", "Line" => "線",
            "RoundedRectangle" => "角丸長方形", "Diamond (Decision)" => "ひし形（判断）",
            "Connector" => "コネクター", "Orthogonal Connector" => "直交コネクター",
            "More ▾" => "その他 ▾", "Color ▾" => "色 ▾", "Language" => "言語",
            "Export Image" => "画像を出力", "Save as PNG…" => "PNG で保存…",
            "Save as SVG…" => "SVG で保存…", "Copy Image" => "画像をコピー",
            "Find Node…  Ctrl+F" => "ノードを検索…  Ctrl+F",
            "Show Dot Grid" => "ドット グリッドを表示", "Grid Settings…" => "グリッド設定…",
            "Automatically rearrange when adding" => "追加時に自動整列する",
            "Snap to Grid" => "グリッドにスナップ", "Light Theme" => "ライトテーマ", "Dark Theme" => "ダークテーマ",
            "Align" => "整列", "Distribute Evenly" => "均等に配置",
            "Distribute Horizontally" => "水平方向に均等配置", "Distribute Vertically" => "垂直方向に均等配置",
            "Create Frame from Selection" => "選択範囲からフレームを作成",
            "Remove from Frame" => "フレームから削除", "Duplicate Ctrl+D" => "複製  Ctrl+D",
            "Arrange Order" => "重なり順", "Bring to Front" => "最前面へ", "Bring Forward" => "前面へ",
            "Send Backward" => "背面へ", "Send to Back" => "最背面へ",
            "More Shapes ▾" => "その他の図形 ▾", "Shapes ▾" => "図形 ▾", "Lines ▾" => "線 ▾",
            "Parallelogram (Input/Output)" => "平行四辺形（入出力）",
            "Cylinder (Database)" => "円柱（データベース）", "Hexagon (Preparation)" => "六角形（準備）",
            "Document" => "書類", "Line (Freeform)" => "線（フリーハンド）",
            "Straight Connector" => "直線コネクター", "Orthogonal ConnectorLines" => "直交コネクター",
            "Polyline Connector" => "折れ線コネクター",
            "Connector (Arrow)" => "コネクター (矢印)", "Connector ▾" => "コネクター ▾",
            "Copy Selection JSON" => "選択範囲 JSON をコピー", "Selection JSON" => "選択範囲 JSON",
            "Left" => "左揃え", "Center" => "中央揃え", "Right" => "右揃え",
            "Top" => "上揃え", "Middle" => "中央揃え", "Bottom" => "下揃え",
            _ => english,
        }
    }
}
