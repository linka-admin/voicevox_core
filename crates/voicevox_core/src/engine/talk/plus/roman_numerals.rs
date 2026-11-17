// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/utils.py`: `normalize_roman_numerals`, `_ROMAN_NUMERAL_VALUES`)

//! ローマ数字の専用文字を、同じ文字数の全角の算用数字へ置き換える。

use std::collections::HashSet;

use super::mecab_morphs::{mecab_index_by_original_index, morphs};

/// ローマ数字の専用文字を、同じ文字数の全角の算用数字へ置き換える。
///
/// - `normalize_for_mecab`: Open JTalkの`text2mecab`による正規化。
/// - `analyze`: 入力をMeCabで解析し、特徴量文字列（`表層形,品詞,...`、空白も含む）を返す。
///   ローマ数字を含まない入力では呼ばれない。
///
/// 「Ⅰ章」「Ⅱ日」「ⅩⅩ」は、ローマ数字のままでは1字ずつ数詞の行に置き換わるだけで助数詞の処理
/// を通らず、「イチショー」「ニニチ」「ジュージュー」と読まれる。算用数字へ置き換えると「イッショー」
/// 「フツカ」「ニジュー」のように、算用数字と同じ音便と和語の読みになる。値の桁数が文字数と合わない
/// 「Ⅹ」(10)や「Ⅻ」(12)、「ⅩⅩⅩ」(30)は、辞書の読みのまま残す。
///
/// ユーザー辞書に登録した「Ⅲ型」のような語の中のローマ数字は、置き換えると登録した語に当たらなく
/// なるので、置き換えない。pyopenjtalk-plusはユーザー辞書を読み込んでいるときだけ解析するが、ここでは
/// ユーザー辞書の有無が分からないので、ローマ数字を含む入力では常に解析する。
pub(crate) fn normalize_roman_numerals(
    text: &str,
    normalize_for_mecab: impl Fn(&str) -> anyhow::Result<String>,
    analyze: impl FnOnce(&str) -> anyhow::Result<Vec<String>>,
) -> anyhow::Result<String> {
    // ローマ数字を含まない通常の文では、追加の形態素解析をしない
    if !text.chars().any(|c| value_of(c).is_some()) {
        return Ok(text.to_owned());
    }
    let characters = text.chars().collect::<Vec<_>>();

    // ユーザー辞書の語の中にあるローマ数字の位置を集め、その語の登録した読みを保つため置き換えない
    let features = analyze(text)?;
    let mut kept_indices = HashSet::new();
    let mut kept_characters = HashSet::new();
    let normalized_text = normalize_for_mecab(text)?;
    let user_dictionary_morphs = morphs(&features, &normalized_text)
        .unwrap_or_default()
        .into_iter()
        .filter(|morph| !morph.is_unknown && morph.is_user_dictionary)
        .collect::<Vec<_>>();
    match mecab_index_by_original_index(text, &normalize_for_mecab)? {
        Some(mecab_indices) => {
            let user_dictionary_mecab_indices = user_dictionary_morphs
                .iter()
                .flat_map(|morph| morph.span.clone())
                .collect::<HashSet<_>>();
            kept_indices = (0..characters.len())
                .filter(|&index| {
                    value_of(characters[index]).is_some()
                        && user_dictionary_mecab_indices.contains(&mecab_indices[index])
                })
                .collect();
        }
        None => {
            // 位置を対応付けられない入力では、ユーザー辞書の語に現れるローマ数字の字をどこでも置き換えない
            kept_characters = user_dictionary_morphs
                .iter()
                .flat_map(|morph| morph.surface.chars())
                .filter(|&c| value_of(c).is_some())
                .collect();
        }
    }
    let is_replaceable = |index: usize| {
        let c = characters[index];
        value_of(c).is_some() && !kept_indices.contains(&index) && !kept_characters.contains(&c)
    };

    let mut result = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        if !is_replaceable(index) {
            result.push(characters[index]);
            index += 1;
            continue;
        }
        let mut end = index;
        while end < characters.len() && is_replaceable(end) {
            end += 1;
        }
        let numeral = &characters[index..end];
        // 「パートⅡ５階」の建物番号と階数は別の数なので、隣に算用数字がある並びは元の文字で区切りを保つ
        let is_adjacent_digit = (index > 0 && is_digit(characters[index - 1]))
            || (end < characters.len() && is_digit(characters[end]));
        match numeral_value(numeral) {
            Some(value) if value.to_string().len() == numeral.len() && !is_adjacent_digit => {
                result.extend(value.to_string().chars().map(|digit| {
                    char::from_u32(u32::from(digit) - u32::from('0') + u32::from('０'))
                        .expect("should be a fullwidth digit")
                }));
            }
            _ => result.extend(numeral),
        }
        index = end;
    }
    Ok(result)
}

/// ローマ数字の専用文字（大文字の「Ⅰ」〜「Ⅿ」と小文字の「ⅰ」〜「ⅿ」）の値。
fn value_of(c: char) -> Option<u32> {
    const VALUES: [u32; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 50, 100, 500, 1000];
    match c {
        'Ⅰ'..='Ⅿ' => Some(VALUES[(u32::from(c) - u32::from('Ⅰ')) as usize]),
        'ⅰ'..='ⅿ' => Some(VALUES[(u32::from(c) - u32::from('ⅰ')) as usize]),
        _ => None,
    }
}

/// ローマ数字の専用文字を、NFKCで分解したラテン文字（大文字）で表す。
fn latin_letters(c: char) -> &'static str {
    const LETTERS: [&str; 16] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "L", "C", "D",
        "M",
    ];
    match c {
        'Ⅰ'..='Ⅿ' => LETTERS[(u32::from(c) - u32::from('Ⅰ')) as usize],
        'ⅰ'..='ⅿ' => LETTERS[(u32::from(c) - u32::from('ⅰ')) as usize],
        _ => unreachable!("should be a roman numeral"),
    }
}

/// ローマ数字の並びを値へ変換する。「ⅠⅤⅩ」のように標準の書き方でない並びは`None`。
fn numeral_value(numeral: &[char]) -> Option<u32> {
    let values = numeral
        .iter()
        .map(|&c| value_of(c).expect("should be a roman numeral"))
        .collect::<Vec<_>>();
    let value = values
        .iter()
        .enumerate()
        .map(|(index, &current)| {
            if values.get(index + 1).is_some_and(|&next| current < next) {
                -i64::from(current)
            } else {
                i64::from(current)
            }
        })
        .sum::<i64>();
    let value = u32::try_from(value).ok()?;

    // 値から作り直した標準の書き方と一致する並びだけを数として読む
    let mut canonical = String::new();
    let mut remaining = value;
    for (symbol_value, symbol) in [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        canonical.push_str(&symbol.repeat((remaining / symbol_value) as usize));
        remaining %= symbol_value;
    }
    let letters = numeral
        .iter()
        .map(|&c| latin_letters(c))
        .collect::<String>();
    (letters == canonical).then_some(value)
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit() || ('０'..='９').contains(&c)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    fn identity(s: &str) -> anyhow::Result<String> {
        Ok(s.to_owned())
    }

    #[rstest]
    fn does_not_analyze_text_without_roman_numerals() {
        let actual =
            super::normalize_roman_numerals("1章", identity, |_| panic!("should not analyze"))
                .unwrap();
        assert_eq!("1章", actual);
    }

    #[rstest]
    #[case("Ⅰ章", "１章")]
    #[case("Ⅱ日", "２日")]
    #[case("ⅩⅩ", "２０")]
    #[case("ⅹⅹ", "２０")]
    #[case("ⅩⅡ", "１２")]
    #[case("第Ⅰ期", "第１期")]
    #[case("ロッキーⅣ", "ロッキー４")]
    #[case("ⅩⅠ", "１１")]
    #[case("ⅯⅭⅯⅬⅩⅩⅩⅣ", "ⅯⅭⅯⅬⅩⅩⅩⅣ")]
    // 値の桁数が文字数と合わない
    #[case("Ⅹ", "Ⅹ")]
    #[case("第Ⅺ章", "第Ⅺ章")]
    #[case("ⅻ型", "ⅻ型")]
    #[case("ⅩⅩⅩ", "ⅩⅩⅩ")]
    #[case("ⅯⅯⅩⅩⅥ", "ⅯⅯⅩⅩⅥ")]
    // 標準の書き方でない
    #[case("ⅠⅤⅩ", "ⅠⅤⅩ")]
    #[case("ⅠⅠⅠⅠ", "ⅠⅠⅠⅠ")]
    // 隣に算用数字がある
    #[case("パートⅡ５階", "パートⅡ５階")]
    #[case("パートⅡ5階", "パートⅡ5階")]
    // 並びごとに置き換える
    #[case("Ⅰ章とⅡ章", "１章と２章")]
    fn normalize_roman_numerals_works(#[case] text: &str, #[case] expected: &str) {
        let actual = super::normalize_roman_numerals(text, identity, |text| {
            // ユーザー辞書の語を含まない解析
            Ok(text
                .chars()
                .map(|c| format!("{c},名詞,数,*,*,*,*,*"))
                .collect())
        })
        .unwrap();
        assert_eq!(expected, actual);
    }

    #[rstest]
    fn keeps_roman_numerals_in_user_dictionary_words() {
        let actual = super::normalize_roman_numerals("Ⅲ型をⅡ日使う", identity, |_| {
            Ok([
                "Ⅲ型,名詞,固有名詞,一般,*,*,*,*,タイプスリー,タイプスリー,3/6,*",
                "を,助詞,格助詞,一般,*,*,*,を,ヲ,ヲ,0/1,*",
                "Ⅱ,名詞,数,*,*,*,*,Ⅱ,ニ,ニ,1/1,*",
                "日,名詞,接尾,助数詞,*,*,*,日,ニチ,ニチ,1/2,C3",
                "使う,動詞,自立,*,*,五段・ワ行促音便,基本形,使う,ツカウ,ツカウ,0/3,C1",
            ]
            .map(ToOwned::to_owned)
            .into())
        })
        .unwrap();
        assert_eq!("Ⅲ型を２日使う", actual);
    }

    #[rstest]
    fn keeps_user_dictionary_characters_when_positions_cannot_be_mapped() {
        let normalize = |s: &str| Ok(s.replace("ab", "Ｘ"));
        let actual = super::normalize_roman_numerals("abⅢ型、Ⅲ", normalize, |_| {
            Ok([
                "Ｘ,名詞,一般,*,*,*,*,*",
                "Ⅲ型,名詞,固有名詞,一般,*,*,*,*,タイプスリー,タイプスリー,3/6,*",
                "、,記号,読点,*,*,*,*,、,、,、,0/0,*",
                "Ⅲ,名詞,数,*,*,*,*,Ⅲ,サン,サン,1/2,*",
            ]
            .map(ToOwned::to_owned)
            .into())
        })
        .unwrap();
        assert_eq!("abⅢ型、Ⅲ", actual);
    }
}
