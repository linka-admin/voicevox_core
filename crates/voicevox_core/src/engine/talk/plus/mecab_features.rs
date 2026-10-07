// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/openjtalk.pyx`: `_expand_symbol_feature`, `_mark_numeral_space_boundaries`,
// `OpenJTalk._run_njd_from_mecab`の未知語の数字列の展開)

//! MeCabの特徴量文字列（`表層形,品詞,...`）に対する規則。

use super::known_symbols::known_symbol_feature;

/// MeCabが1つの未知語にまとめた記号の並びを分けるきっかけとなる記号。
///
/// 疑問符か感嘆符を含む並びだけを分け、踊り字の「ヾ」などを含む顔文字はまとめたままにする。
const PAUSE_SYMBOLS_REQUIRING_EXPANSION: &[char] = &['！', '？'];

/// 未知語の数字列に含まれうる区切り記号。
const NUMERAL_SEPARATORS: &str = "−－ー‐‑‒–—-";

const FULLWIDTH_DIGITS: &str = "０１２３４５６７８９";
const KANJI_DIGITS: &str = "〇一二三四五六七八九";

/// MeCabの解析結果に、NJDへ渡す前の規則を適用する。
///
/// pyopenjtalk-plusの`OpenJTalk._run_mecab`に相当する。
pub(crate) fn apply_mecab_rules(features: &[String]) -> Vec<String> {
    let expanded = features
        .iter()
        .flat_map(|f| expand_symbol_feature(f))
        .collect::<Vec<_>>();
    mark_numeral_space_boundaries(&expanded)
}

/// MeCabが1つの未知語にまとめた記号の並びを、1文字ずつの特徴量に分ける。
///
/// 疑問符か感嘆符を含まない特徴量は分けず、そのまま1件だけ返す。
fn expand_symbol_feature(feature: &str) -> Vec<String> {
    let columns = feature.split(',').collect::<Vec<_>>();
    let surface = columns[0];
    let is_unknown_symbol_chunk = columns.len() == 8
        && surface.chars().count() > 1
        && surface.chars().all(|c| !c.is_alphanumeric())
        && surface
            .chars()
            .any(|c| PAUSE_SYMBOLS_REQUIRING_EXPANSION.contains(&c));
    if !is_unknown_symbol_chunk {
        return vec![feature.to_owned()];
    }

    let rest = columns[1..].join(",");
    surface
        .chars()
        .map(|c| match known_symbol_feature(c) {
            Some(known) => format!("{c},{known}"),
            None => format!("{c},{rest}"),
        })
        .collect()
}

/// 数字間の空白を後続の結合境界へ移し、空白自体はNJDの入力から除く。
///
/// 「EF65 1032号機」の空白は無音のまま、次の数詞の位取りだけを独立させる。
fn mark_numeral_space_boundaries(features: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(features.len());
    let mut previous_was_number = false;
    let mut number_space = false;
    for feature in features {
        if feature.contains("記号,空白") {
            number_space = previous_was_number;
            continue;
        }
        let mut columns = feature
            .split(',')
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let is_number = columns.len() > 2 && columns[1] == "名詞" && columns[2] == "数";
        let feature = if number_space && is_number {
            if columns.len() == 12 {
                columns.push("0".to_owned());
            } else if columns.len() > 12 {
                columns[12] = "0".to_owned();
            }
            columns.join(",")
        } else {
            feature.clone()
        };
        result.push(feature);
        previous_was_number = is_number;
        number_space = false;
    }
    result
}

/// 未知語にまとまった「〇七〇−〇〇二四」のような数字列を、1文字ずつの数詞と区切りに分ける。
///
/// 桁と区切りをNJDの数詞処理へ渡すため。
pub(crate) fn expand_unknown_numeral_chunks(features: &[String]) -> Vec<String> {
    features
        .iter()
        .flat_map(|feature| {
            let columns = feature.split(',').collect::<Vec<_>>();
            let surface = columns[0];
            let is_unknown_numeral_chunk = columns.len() == 8
                && surface.chars().count() > 1
                && surface.chars().all(|c| {
                    FULLWIDTH_DIGITS.contains(c)
                        || KANJI_DIGITS.contains(c)
                        || c == '零'
                        || NUMERAL_SEPARATORS.contains(c)
                })
                && surface.chars().any(|c| {
                    (FULLWIDTH_DIGITS.contains(c) || KANJI_DIGITS.contains(c) || c == '零')
                        && c != '〇'
                });
            if !is_unknown_numeral_chunk {
                return vec![feature.clone()];
            }
            surface.chars().map(numeral_char_feature).collect()
        })
        .collect()
}

fn numeral_char_feature(c: char) -> String {
    if NUMERAL_SEPARATORS.contains(c) {
        return format!("{c},記号,一般,*,*,*,*,{c},、,、,0/0,*");
    }
    let digit = FULLWIDTH_DIGITS
        .chars()
        .position(|d| d == c)
        .or_else(|| KANJI_DIGITS.chars().position(|d| d == c))
        .unwrap_or(0); // 「零」
    const READINGS: [&str; 10] = [
        "ゼロ",
        "イチ",
        "ニ",
        "サン",
        "ヨン",
        "ゴ",
        "ロク",
        "ナナ",
        "ハチ",
        "キュー",
    ];
    // 未知の「二」「五」は1拍で渡し、電話番号の「ニー」「ゴー」への伸長はNJDが決める
    const MORA_SIZES: [u8; 10] = [2, 2, 1, 2, 2, 1, 2, 2, 2, 2];
    let reading = READINGS[digit];
    let mora_size = MORA_SIZES[digit];
    format!("{c},名詞,数,*,*,*,*,{c},{reading},{reading},0/{mora_size},C3")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    fn owned(features: &[&str]) -> Vec<String> {
        features.iter().map(|&s| s.to_owned()).collect()
    }

    #[rstest]
    #[case(
        "！？！？,記号,一般,*,*,*,*,*",
        &[
            "！,記号,一般,*,*,*,*,！,！,！,*/*,*",
            "？,記号,一般,*,*,*,*,？,？,？,*/*,*",
            "！,記号,一般,*,*,*,*,！,！,！,*/*,*",
            "？,記号,一般,*,*,*,*,？,？,？,*/*,*",
        ]
    )]
    // 疑問符も感嘆符も含まない顔文字はまとめたまま
    #[case("ヾ(・・),記号,一般,*,*,*,*,*", &["ヾ(・・),記号,一般,*,*,*,*,*"])]
    // 辞書にない記号は未知語の品詞のまま分ける
    #[case(
        "！✨,記号,一般,*,*,*,*,*",
        &["！,記号,一般,*,*,*,*,！,！,！,*/*,*", "✨,記号,一般,*,*,*,*,*"]
    )]
    // 既知語（12列）は分けない
    #[case("！,記号,一般,*,*,*,*,！,！,！,*/*,*", &["！,記号,一般,*,*,*,*,！,！,！,*/*,*"])]
    // 英数字を含む未知語は分けない
    #[case("Ａ！？,名詞,一般,*,*,*,*,*", &["Ａ！？,名詞,一般,*,*,*,*,*"])]
    fn expand_symbol_feature_works(#[case] feature: &str, #[case] expected: &[&str]) {
        assert_eq!(owned(expected), super::expand_symbol_feature(feature));
    }

    #[rstest]
    // 「EF65 1032号機」: 数字間の空白は消え、直後の数詞が独立する
    #[case(
        &[
            "ＥＦ,名詞,一般,*,*,*,*,ＥＦ,イーエフ,イーエフ,3/4,C1",
            "６,名詞,数,*,*,*,*,６,ロク,ロク,2/2,C3",
            "５,名詞,数,*,*,*,*,５,ゴ,ゴ,1/1,C3",
            "　,記号,空白,*,*,*,*,　,　,　,*/*,*",
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3",
            "０,名詞,数,*,*,*,*,０,ゼロ,ゼロ,1/2,C1",
        ],
        &[
            "ＥＦ,名詞,一般,*,*,*,*,ＥＦ,イーエフ,イーエフ,3/4,C1",
            "６,名詞,数,*,*,*,*,６,ロク,ロク,2/2,C3",
            "５,名詞,数,*,*,*,*,５,ゴ,ゴ,1/1,C3",
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3,0",
            "０,名詞,数,*,*,*,*,０,ゼロ,ゼロ,1/2,C1",
        ]
    )]
    // 数字の後でない空白は単に除く
    #[case(
        &[
            "猫,名詞,一般,*,*,*,*,猫,ネコ,ネコ,1/2,C4",
            "　,記号,空白,*,*,*,*,　,　,　,*/*,*",
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3",
        ],
        &[
            "猫,名詞,一般,*,*,*,*,猫,ネコ,ネコ,1/2,C4",
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3",
        ]
    )]
    // 13列目が既にある場合は上書きする
    #[case(
        &[
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3",
            "　,記号,空白,*,*,*,*,　,　,　,*/*,*",
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3,1",
        ],
        &[
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3",
            "１,名詞,数,*,*,*,*,１,イチ,イチ,2/2,C3,0",
        ]
    )]
    fn mark_numeral_space_boundaries_works(#[case] features: &[&str], #[case] expected: &[&str]) {
        assert_eq!(
            owned(expected),
            super::mark_numeral_space_boundaries(&owned(features)),
        );
    }

    #[rstest]
    fn apply_mecab_rules_expands_symbols_before_dropping_spaces() {
        assert_eq!(
            owned(&[
                "マジ,名詞,形容動詞語幹,*,*,*,*,マジ,マジ,マジ,1/2,C3",
                "！,記号,一般,*,*,*,*,！,！,！,*/*,*",
                "？,記号,一般,*,*,*,*,？,？,？,*/*,*",
            ]),
            super::apply_mecab_rules(&owned(&[
                "マジ,名詞,形容動詞語幹,*,*,*,*,マジ,マジ,マジ,1/2,C3",
                "　,記号,空白,*,*,*,*,　,　,　,*/*,*",
                "！？,記号,一般,*,*,*,*,*",
            ])),
        );
    }

    #[rstest]
    #[case(
        "〇七〇−〇〇二四,名詞,サ変接続,*,*,*,*,*",
        &[
            "〇,名詞,数,*,*,*,*,〇,ゼロ,ゼロ,0/2,C3",
            "七,名詞,数,*,*,*,*,七,ナナ,ナナ,0/2,C3",
            "〇,名詞,数,*,*,*,*,〇,ゼロ,ゼロ,0/2,C3",
            "−,記号,一般,*,*,*,*,−,、,、,0/0,*",
            "〇,名詞,数,*,*,*,*,〇,ゼロ,ゼロ,0/2,C3",
            "〇,名詞,数,*,*,*,*,〇,ゼロ,ゼロ,0/2,C3",
            "二,名詞,数,*,*,*,*,二,ニ,ニ,0/1,C3",
            "四,名詞,数,*,*,*,*,四,ヨン,ヨン,0/2,C3",
        ]
    )]
    #[case("零５,名詞,数,*,*,*,*,*", &[
        "零,名詞,数,*,*,*,*,零,ゼロ,ゼロ,0/2,C3",
        "５,名詞,数,*,*,*,*,５,ゴ,ゴ,0/1,C3",
    ])]
    // 〇だけの並びは伏字として残す
    #[case("〇〇〇,名詞,サ変接続,*,*,*,*,*", &["〇〇〇,名詞,サ変接続,*,*,*,*,*"])]
    // 既知語は分けない
    #[case(
        "１２,名詞,数,*,*,*,*,１２,ジュウニ,ジューニ,1/3,C3",
        &["１２,名詞,数,*,*,*,*,１２,ジュウニ,ジューニ,1/3,C3"]
    )]
    // 1文字だけの未知語は分けない
    #[case("七,名詞,数,*,*,*,*,*", &["七,名詞,数,*,*,*,*,*"])]
    fn expand_unknown_numeral_chunks_works(#[case] feature: &str, #[case] expected: &[&str]) {
        assert_eq!(
            owned(expected),
            super::expand_unknown_numeral_chunks(&owned(&[feature])),
        );
    }
}
