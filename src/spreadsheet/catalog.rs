//! Static metadata for formula autocomplete and argument help.
//!
//! Keep this catalog independent of the evaluator so the editor can show
//! useful candidates before formula evaluation is complete. While candidates
//! are visible, Tab inserts the selected function; Enter commits the cell and
//! moves down instead of accepting a candidate.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionCategory {
    MathAndAggregate,
    ConditionalAggregate,
    LogicalAndErrors,
    Text,
    DateAndTime,
    StatisticsAndArrays,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgumentSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub optional: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionSpec {
    pub canonical_name: &'static str,
    pub aliases: &'static [&'static str],
    pub category: FunctionCategory,
    pub japanese_description: &'static str,
    pub signature: &'static str,
    pub arguments: &'static [ArgumentSpec],
    pub example: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionCandidate {
    /// Spelling inserted into the formula. This can be an alias.
    pub insert_name: &'static str,
    pub function: &'static FunctionSpec,
}

const fn arg(name: &'static str, description: &'static str, optional: bool) -> ArgumentSpec {
    ArgumentSpec {
        name,
        description,
        optional,
    }
}

pub static FUNCTIONS: &[FunctionSpec] = &[
    FunctionSpec {
        canonical_name: "SUM",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "数値を合計します。",
        signature: "SUM(number1, [number2], …)",
        arguments: &[
            arg("number1", "合計する数値または範囲", false),
            arg("number2", "追加する数値または範囲", true),
        ],
        example: "=SUM(A1:A5)",
    },
    FunctionSpec {
        canonical_name: "AVERAGE",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "数値の平均を求めます。",
        signature: "AVERAGE(number1, [number2], …)",
        arguments: &[
            arg("number1", "平均を求める数値または範囲", false),
            arg("number2", "追加する数値または範囲", true),
        ],
        example: "=AVERAGE(B2:B10)",
    },
    FunctionSpec {
        canonical_name: "ROUND",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "指定した桁数に四捨五入します。",
        signature: "ROUND(number, digits)",
        arguments: &[
            arg("number", "丸める数値", false),
            arg("digits", "丸める桁数。負数で小数点より左を丸めます", false),
        ],
        example: "=ROUND(12.345, 2)",
    },
    FunctionSpec {
        canonical_name: "ROUNDDOWN",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "数値を0に近づく方向へ切り捨てます。",
        signature: "ROUNDDOWN(number, digits)",
        arguments: &[
            arg("number", "丸める数値", false),
            arg("digits", "丸める桁数", false),
        ],
        example: "=ROUNDDOWN(-12.34, 1)",
    },
    FunctionSpec {
        canonical_name: "ROUNDUP",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "数値を0から遠ざかる方向へ切り上げます。",
        signature: "ROUNDUP(number, digits)",
        arguments: &[
            arg("number", "丸める数値", false),
            arg("digits", "丸める桁数", false),
        ],
        example: "=ROUNDUP(12.341, 2)",
    },
    FunctionSpec {
        canonical_name: "SUBTOTAL",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "フィルターで除外された行を除いて集計します。",
        signature: "SUBTOTAL(function_num, ref1, [ref2], …)",
        arguments: &[
            arg(
                "function_num",
                "集計方法の番号（1〜11または101〜111）",
                false,
            ),
            arg("ref1", "集計する最初の範囲", false),
            arg("ref2", "追加する範囲", true),
        ],
        example: "=SUBTOTAL(9, C2:C20)",
    },
    FunctionSpec {
        canonical_name: "AGGREGATE",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "非表示行やエラー値を指定方法で除外して集計します。",
        signature: "AGGREGATE(function_num, options, ref1, [ref2], …)",
        arguments: &[
            arg("function_num", "集計方法の番号（1〜19）", false),
            arg("options", "非表示行・エラー等を無視する方法（0〜7）", false),
            arg("ref1 / array", "集計範囲または配列", false),
            arg(
                "k",
                "LARGE/SMALL/順位・百分位関数で使う順位または引数",
                true,
            ),
        ],
        example: "=AGGREGATE(9, 6, A1:A20)",
    },
    FunctionSpec {
        canonical_name: "PRODUCT",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "数値をすべて掛け合わせます。",
        signature: "PRODUCT(number1, [number2], …)",
        arguments: &[
            arg("number1", "乗算する数値または範囲", false),
            arg("number2", "追加する数値または範囲", true),
        ],
        example: "=PRODUCT(A1:A4)",
    },
    FunctionSpec {
        canonical_name: "MOD",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "割り算の余りを返します。",
        signature: "MOD(number, divisor)",
        arguments: &[
            arg("number", "割られる数値", false),
            arg("divisor", "除数", false),
        ],
        example: "=MOD(17, 5)",
    },
    FunctionSpec {
        canonical_name: "ABS",
        aliases: &[],
        category: FunctionCategory::MathAndAggregate,
        japanese_description: "数値の絶対値を返します。",
        signature: "ABS(number)",
        arguments: &[arg("number", "絶対値を求める数値", false)],
        example: "=ABS(-8.5)",
    },
    FunctionSpec {
        canonical_name: "SUMIF",
        aliases: &[],
        category: FunctionCategory::ConditionalAggregate,
        japanese_description: "1つの条件に一致する値を合計します。",
        signature: "SUMIF(range, criteria, [sum_range])",
        arguments: &[
            arg("range", "条件を調べるセル範囲", false),
            arg("criteria", "一致条件", false),
            arg("sum_range", "合計対象範囲。省略時はrange", true),
        ],
        example: "=SUMIF(A2:A20, \"東京\", C2:C20)",
    },
    FunctionSpec {
        canonical_name: "SUMIFS",
        aliases: &[],
        category: FunctionCategory::ConditionalAggregate,
        japanese_description: "複数の条件すべてに一致する値を合計します。",
        signature: "SUMIFS(sum_range, criteria_range1, criteria1, …)",
        arguments: &[
            arg("sum_range", "合計するセル範囲", false),
            arg("criteria_range1", "最初の条件範囲", false),
            arg("criteria1", "最初の条件", false),
            arg(
                "criteria_rangeN / criteriaN",
                "追加の条件範囲と条件の組",
                true,
            ),
        ],
        example: "=SUMIFS(C2:C20, A2:A20, \"東京\", B2:B20, \">=10\")",
    },
    FunctionSpec {
        canonical_name: "COUNTIF",
        aliases: &[],
        category: FunctionCategory::ConditionalAggregate,
        japanese_description: "1つの条件に一致するセルの数を数えます。",
        signature: "COUNTIF(range, criteria)",
        arguments: &[
            arg("range", "条件を調べるセル範囲", false),
            arg("criteria", "一致条件", false),
        ],
        example: "=COUNTIF(B2:B20, \">=80\")",
    },
    FunctionSpec {
        canonical_name: "COUNTIFS",
        aliases: &[],
        category: FunctionCategory::ConditionalAggregate,
        japanese_description: "複数の条件すべてに一致するセルの数を数えます。",
        signature: "COUNTIFS(criteria_range1, criteria1, …)",
        arguments: &[
            arg("criteria_range1", "最初の条件範囲", false),
            arg("criteria1", "最初の条件", false),
            arg(
                "criteria_rangeN / criteriaN",
                "追加の条件範囲と条件の組",
                true,
            ),
        ],
        example: "=COUNTIFS(A2:A20, \"東京\", B2:B20, \">=10\")",
    },
    FunctionSpec {
        canonical_name: "AVERAGEIF",
        aliases: &[],
        category: FunctionCategory::ConditionalAggregate,
        japanese_description: "1つの条件に一致する値の平均を求めます。",
        signature: "AVERAGEIF(range, criteria, [average_range])",
        arguments: &[
            arg("range", "条件を調べるセル範囲", false),
            arg("criteria", "一致条件", false),
            arg("average_range", "平均対象範囲。省略時はrange", true),
        ],
        example: "=AVERAGEIF(A2:A20, \"東京\", C2:C20)",
    },
    FunctionSpec {
        canonical_name: "AVERAGEIFS",
        aliases: &[],
        category: FunctionCategory::ConditionalAggregate,
        japanese_description: "複数の条件すべてに一致する値の平均を求めます。",
        signature: "AVERAGEIFS(average_range, criteria_range1, criteria1, …)",
        arguments: &[
            arg("average_range", "平均するセル範囲", false),
            arg("criteria_range1", "最初の条件範囲", false),
            arg("criteria1", "最初の条件", false),
            arg(
                "criteria_rangeN / criteriaN",
                "追加の条件範囲と条件の組",
                true,
            ),
        ],
        example: "=AVERAGEIFS(C2:C20, A2:A20, \"東京\", B2:B20, \">=10\")",
    },
    FunctionSpec {
        canonical_name: "IF",
        aliases: &[],
        category: FunctionCategory::LogicalAndErrors,
        japanese_description: "条件が真か偽かで返す値を切り替えます。",
        signature: "IF(logical_test, value_if_true, [value_if_false])",
        arguments: &[
            arg("logical_test", "真偽を判定する条件", false),
            arg("value_if_true", "条件が真の場合の値", false),
            arg("value_if_false", "条件が偽の場合の値。省略時はFALSE", true),
        ],
        example: "=IF(A1>=60, \"合格\", \"不合格\")",
    },
    FunctionSpec {
        canonical_name: "IFS",
        aliases: &[],
        category: FunctionCategory::LogicalAndErrors,
        japanese_description: "複数条件を順番に調べ、最初に真となる値を返します。",
        signature: "IFS(logical_test1, value_if_true1, …)",
        arguments: &[
            arg("logical_test1", "最初に調べる条件", false),
            arg("value_if_true1", "最初の条件が真の場合の値", false),
            arg("test/value pairs", "追加の条件と結果の組", true),
        ],
        example: "=IFS(A1>=80, \"A\", A1>=60, \"B\", TRUE, \"C\")",
    },
    FunctionSpec {
        canonical_name: "AND",
        aliases: &[],
        category: FunctionCategory::LogicalAndErrors,
        japanese_description: "すべての条件が真かを判定します。",
        signature: "AND(logical1, [logical2], …)",
        arguments: &[
            arg("logical1", "判定する最初の条件", false),
            arg("logical2", "追加の条件", true),
        ],
        example: "=AND(A1>0, B1<10)",
    },
    FunctionSpec {
        canonical_name: "OR",
        aliases: &[],
        category: FunctionCategory::LogicalAndErrors,
        japanese_description: "いずれかの条件が真かを判定します。",
        signature: "OR(logical1, [logical2], …)",
        arguments: &[
            arg("logical1", "判定する最初の条件", false),
            arg("logical2", "追加の条件", true),
        ],
        example: "=OR(A1=\"済\", B1=\"済\")",
    },
    FunctionSpec {
        canonical_name: "NOT",
        aliases: &[],
        category: FunctionCategory::LogicalAndErrors,
        japanese_description: "条件の真偽を反転します。",
        signature: "NOT(logical)",
        arguments: &[arg("logical", "真偽を反転する条件", false)],
        example: "=NOT(A1=0)",
    },
    FunctionSpec {
        canonical_name: "IFERROR",
        aliases: &[],
        category: FunctionCategory::LogicalAndErrors,
        japanese_description: "計算がエラーになった場合に代わりの値を返します。",
        signature: "IFERROR(value, value_if_error)",
        arguments: &[
            arg("value", "計算する式または値", false),
            arg("value_if_error", "エラー時に返す値", false),
        ],
        example: "=IFERROR(A1/B1, 0)",
    },
    FunctionSpec {
        canonical_name: "CONCAT",
        aliases: &["CONCATENATE"],
        category: FunctionCategory::Text,
        japanese_description: "複数の文字列やセルの内容をつなげます。",
        signature: "CONCAT(text1, [text2], …)",
        arguments: &[
            arg("text1", "最初の文字列または範囲", false),
            arg("text2", "追加する文字列または範囲", true),
        ],
        example: "=CONCAT(A1, \" \", B1)",
    },
    FunctionSpec {
        canonical_name: "TEXTJOIN",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "区切り文字を挟んで文字列をつなげます。",
        signature: "TEXTJOIN(delimiter, ignore_empty, text1, …)",
        arguments: &[
            arg("delimiter", "文字列の間に挿入する区切り", false),
            arg("ignore_empty", "空セルを無視するか", false),
            arg("text1", "結合する文字列または範囲", false),
            arg("textN", "追加する文字列または範囲", true),
        ],
        example: "=TEXTJOIN(\", \", TRUE, A1:A5)",
    },
    FunctionSpec {
        canonical_name: "LEFT",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "文字列の先頭から指定文字数を取り出します。",
        signature: "LEFT(text, [num_chars])",
        arguments: &[
            arg("text", "元の文字列", false),
            arg("num_chars", "取り出す文字数。省略時は1", true),
        ],
        example: "=LEFT(A1, 3)",
    },
    FunctionSpec {
        canonical_name: "RIGHT",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "文字列の末尾から指定文字数を取り出します。",
        signature: "RIGHT(text, [num_chars])",
        arguments: &[
            arg("text", "元の文字列", false),
            arg("num_chars", "取り出す文字数。省略時は1", true),
        ],
        example: "=RIGHT(A1, 4)",
    },
    FunctionSpec {
        canonical_name: "MID",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "文字列の指定位置から文字を取り出します。",
        signature: "MID(text, start_num, num_chars)",
        arguments: &[
            arg("text", "元の文字列", false),
            arg("start_num", "取り出し開始位置（1始まり）", false),
            arg("num_chars", "取り出す文字数", false),
        ],
        example: "=MID(A1, 2, 3)",
    },
    FunctionSpec {
        canonical_name: "LEN",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "文字列の文字数を数えます。",
        signature: "LEN(text)",
        arguments: &[arg("text", "文字数を数える文字列", false)],
        example: "=LEN(A1)",
    },
    FunctionSpec {
        canonical_name: "FIND",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "大文字と小文字を区別して文字列を検索します。",
        signature: "FIND(find_text, within_text, [start_num])",
        arguments: &[
            arg("find_text", "検索する文字列", false),
            arg("within_text", "検索対象の文字列", false),
            arg("start_num", "検索開始位置（1始まり）。省略時は1", true),
        ],
        example: "=FIND(\"cat\", A1)",
    },
    FunctionSpec {
        canonical_name: "SEARCH",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "大文字と小文字を区別せず文字列を検索します。*、?、~を使えます。",
        signature: "SEARCH(find_text, within_text, [start_num])",
        arguments: &[
            arg("find_text", "検索する文字列またはワイルドカード", false),
            arg("within_text", "検索対象の文字列", false),
            arg("start_num", "検索開始位置（1始まり）。省略時は1", true),
        ],
        example: "=SEARCH(\"a*\", A1)",
    },
    FunctionSpec {
        canonical_name: "SUBSTITUTE",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "文字列中の指定した文字列を置き換えます。",
        signature: "SUBSTITUTE(text, old_text, new_text, [instance_num])",
        arguments: &[
            arg("text", "置換対象の文字列", false),
            arg("old_text", "探す文字列", false),
            arg("new_text", "置換後の文字列", false),
            arg("instance_num", "置換する出現回。省略時はすべて", true),
        ],
        example: "=SUBSTITUTE(A1, \"旧\", \"新\")",
    },
    FunctionSpec {
        canonical_name: "REPLACE",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "文字列の指定位置から指定文字数を置き換えます。",
        signature: "REPLACE(old_text, start_num, num_chars, new_text)",
        arguments: &[
            arg("old_text", "置換対象の文字列", false),
            arg("start_num", "置換開始位置（1始まり）", false),
            arg("num_chars", "置換する文字数", false),
            arg("new_text", "挿入する文字列", false),
        ],
        example: "=REPLACE(A1, 1, 3, \"新\")",
    },
    FunctionSpec {
        canonical_name: "TRIM",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "前後の空白を除き、連続する半角スペースを1つにします。",
        signature: "TRIM(text)",
        arguments: &[arg("text", "空白を整理する文字列", false)],
        example: "=TRIM(A1)",
    },
    FunctionSpec {
        canonical_name: "TEXT",
        aliases: &[],
        category: FunctionCategory::Text,
        japanese_description: "数値や日付を指定した書式の文字列にします。",
        signature: "TEXT(value, format_text)",
        arguments: &[
            arg("value", "書式を適用する数値または日付", false),
            arg("format_text", "表示書式", false),
        ],
        example: "=TEXT(A1, \"#,##0.00\")",
    },
    FunctionSpec {
        canonical_name: "TODAY",
        aliases: &[],
        category: FunctionCategory::DateAndTime,
        japanese_description: "今日の日付を返します。",
        signature: "TODAY()",
        arguments: &[],
        example: "=TODAY()",
    },
    FunctionSpec {
        canonical_name: "NOW",
        aliases: &[],
        category: FunctionCategory::DateAndTime,
        japanese_description: "現在の日付と時刻を返します。",
        signature: "NOW()",
        arguments: &[],
        example: "=NOW()",
    },
    FunctionSpec {
        canonical_name: "DATE",
        aliases: &[],
        category: FunctionCategory::DateAndTime,
        japanese_description: "年、月、日から日付を作ります。",
        signature: "DATE(year, month, day)",
        arguments: &[
            arg("year", "年", false),
            arg("month", "月。範囲外は年に繰り上げ", false),
            arg("day", "日。範囲外は月に繰り上げ", false),
        ],
        example: "=DATE(2026, 11, 12)",
    },
    FunctionSpec {
        canonical_name: "DATEDIF",
        aliases: &[],
        category: FunctionCategory::DateAndTime,
        japanese_description: "2つの日付の期間を指定単位で求めます。",
        signature: "DATEDIF(start_date, end_date, unit)",
        arguments: &[
            arg("start_date", "開始日", false),
            arg("end_date", "終了日", false),
            arg("unit", "単位: Y、M、D、YM、YD、MD", false),
        ],
        example: "=DATEDIF(A1, TODAY(), \"Y\")",
    },
    FunctionSpec {
        canonical_name: "NETWORKDAYS",
        aliases: &[],
        category: FunctionCategory::DateAndTime,
        japanese_description: "土日と指定した祝日を除く稼働日数を数えます。",
        signature: "NETWORKDAYS(start_date, end_date, [holidays])",
        arguments: &[
            arg("start_date", "開始日", false),
            arg("end_date", "終了日", false),
            arg("holidays", "除外する祝日の範囲", true),
        ],
        example: "=NETWORKDAYS(A1, B1, H1:H10)",
    },
    FunctionSpec {
        canonical_name: "COUNT",
        aliases: &[],
        category: FunctionCategory::StatisticsAndArrays,
        japanese_description: "数値が含まれるセルの数を数えます。",
        signature: "COUNT(value1, [value2], …)",
        arguments: &[
            arg("value1", "数える値または範囲", false),
            arg("value2", "追加する値または範囲", true),
        ],
        example: "=COUNT(A1:A20)",
    },
    FunctionSpec {
        canonical_name: "COUNTA",
        aliases: &[],
        category: FunctionCategory::StatisticsAndArrays,
        japanese_description: "空でないセルの数を数えます。",
        signature: "COUNTA(value1, [value2], …)",
        arguments: &[
            arg("value1", "数える値または範囲", false),
            arg("value2", "追加する値または範囲", true),
        ],
        example: "=COUNTA(A1:A20)",
    },
    FunctionSpec {
        canonical_name: "RANK.EQ",
        aliases: &[],
        category: FunctionCategory::StatisticsAndArrays,
        japanese_description: "数値が範囲内で何位かを返します。同順位には同じ順位を付けます。",
        signature: "RANK.EQ(number, ref, [order])",
        arguments: &[
            arg("number", "順位を調べる数値", false),
            arg("ref", "比較する値の範囲", false),
            arg("order", "0または省略は降順、1は昇順", true),
        ],
        example: "=RANK.EQ(A1, A1:A10, 0)",
    },
    FunctionSpec {
        canonical_name: "FILTER",
        aliases: &[],
        category: FunctionCategory::StatisticsAndArrays,
        japanese_description: "条件に一致する行または列を抽出します。結果は隣接セルへ展開されます。",
        signature: "FILTER(array, include, [if_empty])",
        arguments: &[
            arg("array", "抽出元の範囲", false),
            arg("include", "各行/列を残すか示す条件", false),
            arg("if_empty", "一致がない場合の値", true),
        ],
        example: "=FILTER(A2:C20, C2:C20>0, \"該当なし\")",
    },
];

/// Return function candidates whose canonical names or aliases start with the
/// typed prefix. Canonical names are returned before aliases for each match.
pub fn candidates(prefix: &str) -> Vec<FunctionCandidate> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for function in FUNCTIONS {
        if starts_with_ascii_case_insensitive(function.canonical_name, prefix) {
            matches.push(FunctionCandidate {
                insert_name: function.canonical_name,
                function,
            });
        }
        for alias in function.aliases {
            if starts_with_ascii_case_insensitive(alias, prefix) {
                matches.push(FunctionCandidate {
                    insert_name: alias,
                    function,
                });
            }
        }
    }
    matches
}

/// Find the identifier fragment at the cursor for formula completion.
/// Returns `None` when the cursor is inside a quoted string. Doubled quotes in
/// Quoted strings support doubled-quote escapes (`"She said ""yes"""`).
pub fn function_prefix_at(source: &str, cursor_byte: usize) -> Option<&str> {
    let cursor = cursor_byte.min(source.len());
    if !source.is_char_boundary(cursor) {
        return None;
    }
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut in_string = false;
    while index < cursor {
        if bytes[index] == b'"' {
            if in_string && index + 1 < cursor && bytes[index + 1] == b'"' {
                index += 2;
                continue;
            }
            in_string = !in_string;
        }
        index += 1;
    }
    if in_string {
        return None;
    }

    let mut start = cursor;
    while start > 0 {
        let byte = bytes[start - 1];
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    if start == cursor {
        return None;
    }
    // Do not offer function names in the middle of an identifier/reference.
    if start > 0 {
        let before = bytes[start - 1];
        if before.is_ascii_alphanumeric() || before == b'_' || before == b'.' {
            return None;
        }
    }
    let prefix = &source[start..cursor];
    prefix
        .bytes()
        .any(|byte| byte.is_ascii_alphabetic())
        .then_some(prefix)
}

fn starts_with_ascii_case_insensitive(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}
