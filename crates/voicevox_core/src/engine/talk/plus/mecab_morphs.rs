// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/utils.py`: `_mecab_index_by_original_index`; `pyopenjtalk/openjtalk.pyx`: `MeCabMorph`)

//! MeCabの前に行う正規化（異体字、ローマ数字）が使う、形態素の位置と出どころ。
//!
//! pyopenjtalk-plusはMeCabのノードから`MeCabMorph`（文字位置、未知語か、何番目の辞書の語か）を作る
//! が、ここではMeCabの特徴量の文字列しか得られないので、次のように復元する。
//!
//! - 文字位置: 表層形を、MeCab向けに正規化した本文の先頭から順に探して求める。
//! - 未知語: 特徴量が12列未満のもの。
//! - ユーザー辞書の語: VOICEVOXのユーザー辞書の語（`UserDictWord::to_mecab_format`）の形、つまり
//!   原形とアクセント結合規則がどちらも`*`のもの。pyopenjtalk-plusのシステム辞書にこの形の語はない。

use std::ops::Range;

/// 形態素のうち、MeCabの前の正規化に使う情報。
#[derive(Debug, PartialEq)]
pub(super) struct Morph<'a> {
    pub(super) surface: &'a str,
    /// MeCab向けに正規化した本文上の、文字単位の半開区間。
    pub(super) span: Range<usize>,
    pub(super) is_unknown: bool,
    pub(super) is_user_dictionary: bool,
}

/// MeCabの特徴量の文字列（`表層形,品詞,...`）から形態素を作る。
///
/// `normalized_text`はMeCab向けに正規化した本文。表層形を本文の中に順に見つけられない場合は`None`。
pub(super) fn morphs<'a>(features: &'a [String], normalized_text: &str) -> Option<Vec<Morph<'a>>> {
    let text = normalized_text.chars().collect::<Vec<_>>();
    let mut cursor = 0;
    features
        .iter()
        .map(|feature| {
            let columns = feature.split(',').collect::<Vec<_>>();
            let surface = columns[0];
            let surface_chars = surface.chars().collect::<Vec<_>>();
            // MeCabが読み飛ばした文字があっても、その後ろから表層形を探す
            let start = (cursor..=text.len().checked_sub(surface_chars.len())?)
                .find(|&start| text[start..start + surface_chars.len()] == surface_chars)?;
            let end = start + surface_chars.len();
            cursor = end;
            Some(Morph {
                surface,
                span: start..end,
                is_unknown: columns.len() < 12,
                is_user_dictionary: columns.len() >= 12 && columns[7] == "*" && columns[11] == "*",
            })
        })
        .collect()
}

/// 入力の各文字が、MeCab向けに正規化した本文の何文字目から始まるかを返す。
///
/// 1文字ずつの正規化と本文全体の正規化が食い違う場合は`None`。
pub(super) fn mecab_index_by_original_index(
    source: &str,
    normalize_for_mecab: impl Fn(&str) -> anyhow::Result<String>,
) -> anyhow::Result<Option<Vec<usize>>> {
    let source = source.chars().collect::<Vec<_>>();

    // 半角濁点は直前のカナと合わせて「ｶﾞ」→「ガ」に縮約されるので、その2文字には同じ開始位置を割り当てる
    let mut chunks = vec![];
    let mut index = 0;
    while index < source.len() {
        let mut end = index + 1;
        if end < source.len() && matches!(source[end], 'ﾞ' | 'ﾟ' | '\u{3099}' | '\u{309A}') {
            end += 1;
        }
        // 異体字セレクターも直前の文字と正規化し、漢字の開始位置へ対応させる
        while end < source.len()
            && matches!(source[end], '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
        {
            end += 1;
        }
        let chunk = source[index..end].iter().collect::<String>();
        chunks.push((end - index, normalize_for_mecab(&chunk)?));
        index = end;
    }
    let concatenated = chunks
        .iter()
        .map(|(_, normalized)| &**normalized)
        .collect::<String>();
    if concatenated != normalize_for_mecab(&source.iter().collect::<String>())? {
        return Ok(None);
    }

    let mut mecab_indices = Vec::with_capacity(source.len());
    let mut mecab_index = 0;
    for (source_size, normalized) in chunks {
        let normalized_size = normalized.chars().count();
        if source_size == normalized_size {
            mecab_indices.extend(mecab_index..mecab_index + source_size);
        } else {
            mecab_indices.extend(std::iter::repeat_n(mecab_index, source_size));
        }
        mecab_index += normalized_size;
    }
    Ok(Some(mecab_indices))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::Morph;

    #[rstest]
    fn morphs_works() {
        let features = [
            "﨑山,名詞,固有名詞,一般,*,*,*,*,テスト,テスト,1/3,*",
            "さん,名詞,接尾,人名,*,*,*,さん,サン,サン,0/2,C3",
            "。,記号,句点,*,*,*,*,。,。,。,0/0,*",
            "﨑,名詞,一般,*,*,*,*,*",
        ]
        .map(ToOwned::to_owned);
        let expected = vec![
            Morph {
                surface: "﨑山",
                span: 1..3,
                is_unknown: false,
                is_user_dictionary: true,
            },
            Morph {
                surface: "さん",
                span: 3..5,
                is_unknown: false,
                is_user_dictionary: false,
            },
            Morph {
                surface: "。",
                span: 5..6,
                is_unknown: false,
                is_user_dictionary: false,
            },
            Morph {
                surface: "﨑",
                span: 6..7,
                is_unknown: true,
                is_user_dictionary: false,
            },
        ];
        // MeCabが読み飛ばした先頭の文字は、どの形態素にも含めない
        assert_eq!(Some(expected), super::morphs(&features, "\t﨑山さん。﨑"));
    }

    #[rstest]
    fn morphs_fails_for_missing_surface() {
        let features = ["亜,名詞,一般,*,*,*,*,*".to_owned()];
        assert_eq!(None, super::morphs(&features, "亞"));
    }

    fn halfwidth_kana(s: &str) -> anyhow::Result<String> {
        Ok(s.replace("ｶﾞ", "ガ").replace('ｲ', "イ").replace(' ', "　"))
    }

    #[rstest]
    #[case("亞 亞", Some(vec![0, 1, 2]))]
    // 縮約される半角濁点は、直前のカナと同じ位置
    #[case("ｶﾞｲ亞", Some(vec![0, 0, 1, 2]))]
    // 異体字セレクターは、直前の漢字と同じ位置
    #[case("﨑\u{FE00}山", Some(vec![0, 1, 2]))]
    fn mecab_index_by_original_index_works(
        #[case] source: &str,
        #[case] expected: Option<Vec<usize>>,
    ) {
        let actual = super::mecab_index_by_original_index(source, halfwidth_kana).unwrap();
        assert_eq!(expected, actual);
    }

    #[rstest]
    fn mecab_index_by_original_index_fails_for_inconsistent_normalization() {
        let normalize = |s: &str| Ok(s.replace("ab", "Ｘ"));
        let actual = super::mecab_index_by_original_index("ab亞", normalize).unwrap();
        assert_eq!(None, actual);
    }
}
