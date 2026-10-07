// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/utils.py`: `normalize_itaiji`, `normalize_unknown_itaiji`)

//! 辞書で読めない異体字だけを通用字へ置き換える。

use std::collections::HashSet;

use super::itaiji_map::tsuyoji;

/// 辞書で読めない異体字だけを通用字へ置き換える。
///
/// - `normalize_for_mecab`: Open JTalkの`text2mecab`による正規化。
/// - `analyze`: 入力をMeCabで解析し、特徴量文字列（`表層形,品詞,...`、空白も含む）を返す。
///   異体字を含まない入力では呼ばれない。
///
/// 辞書（ユーザー辞書を含む）で読める字形は、その字形に固有の読みを残すため置き換えない。
/// 未知語は、特徴量が12列未満のもの。
pub(crate) fn normalize_unknown_itaiji(
    text: &str,
    normalize_for_mecab: impl Fn(&str) -> anyhow::Result<String>,
    analyze: impl FnOnce(&str) -> anyhow::Result<Vec<String>>,
) -> anyhow::Result<String> {
    // 異体字を含まない通常の文では、追加の形態素解析をしない
    if !text.chars().any(|c| tsuyoji(c).is_some()) {
        return Ok(text.to_owned());
    }

    let features = analyze(text)?;
    let morphs = features
        .iter()
        .map(|feature| {
            let columns = feature.split(',').collect::<Vec<_>>();
            (columns[0], columns.len() < 12)
        })
        .collect::<Vec<_>>();

    // 未知語の位置はMeCab向けに正規化した本文上の位置なので、入力の1文字ずつを正規化して入力上の
    // 位置へ対応付ける
    let normalized_characters = text
        .chars()
        .map(|c| normalize_for_mecab(&c.to_string()))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let normalized_text = normalize_for_mecab(text)?;
    let surfaces = morphs
        .iter()
        .map(|&(surface, _)| surface)
        .collect::<String>();
    if normalized_characters.concat() != normalized_text || surfaces != normalized_text {
        // 位置を対応付けられないので、未知語の字の集合で置き換える
        let unknown_characters = morphs
            .iter()
            .filter(|&&(_, is_unknown)| is_unknown)
            .flat_map(|(surface, _)| surface.chars())
            .collect::<HashSet<_>>();
        return Ok(text
            .chars()
            .map(|c| {
                if unknown_characters.contains(&c) {
                    tsuyoji(c).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect());
    }

    let original_index_by_mecab_index = normalized_characters
        .iter()
        .enumerate()
        .flat_map(|(index, normalized)| std::iter::repeat_n(index, normalized.chars().count()))
        .collect::<Vec<_>>();
    let mut unknown_positions = HashSet::<usize>::new();
    let mut mecab_index = 0;
    for (surface, is_unknown) in morphs {
        let len = surface.chars().count();
        if is_unknown {
            unknown_positions
                .extend(&original_index_by_mecab_index[mecab_index..mecab_index + len]);
        }
        mecab_index += len;
    }
    Ok(text
        .chars()
        .enumerate()
        .map(|(index, c)| {
            if unknown_positions.contains(&index) {
                tsuyoji(c).unwrap_or(c)
            } else {
                c
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    fn identity(s: &str) -> anyhow::Result<String> {
        Ok(s.to_owned())
    }

    fn analyzed(features: &[&str]) -> impl FnOnce(&str) -> anyhow::Result<Vec<String>> {
        let features = features.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        move |_| Ok(features)
    }

    #[rstest]
    fn does_not_analyze_text_without_itaiji() {
        let actual = super::normalize_unknown_itaiji("吉野家", identity, |_| {
            panic!("should not analyze")
        })
        .unwrap();
        assert_eq!("吉野家", actual);
    }

    #[rstest]
    #[case(
        "𠮷野家",
        &["𠮷,名詞,一般,*,*,*,*,*", "野家,名詞,固有名詞,*,*,*,*,野家,ノヤ,ノヤ,1/2,C1"],
        "吉野家"
    )]
    // 辞書で読める字形は残す
    #[case(
        "亞細亞",
        &["亞細亞,名詞,固有名詞,地域,国,*,*,亞細亞,アジア,アジア,1/3,C1"],
        "亞細亞"
    )]
    // 同じ異体字でも、未知語の位置にあるものだけを置き換える
    #[case(
        "亞と亞",
        &["亞,名詞,一般,*,*,*,*,亞,ア,ア,1/1,C1", "と,助詞,並立助詞,*,*,*,*,と,ト,ト,0/1,*", "亞,名詞,一般,*,*,*,*,*"],
        "亞と亜"
    )]
    fn replaces_only_unknown_itaiji(
        #[case] text: &str,
        #[case] features: &[&str],
        #[case] expected: &str,
    ) {
        let actual = super::normalize_unknown_itaiji(text, identity, analyzed(features)).unwrap();
        assert_eq!(expected, actual);
    }

    #[rstest]
    fn maps_positions_through_normalization() {
        // 半角空白が全角になってもよい（1文字ずつの正規化と全体の正規化が一致する）
        let normalize = |s: &str| Ok(s.replace(' ', "　"));
        let actual = super::normalize_unknown_itaiji(
            "亞 亞",
            normalize,
            analyzed(&[
                "亞,名詞,一般,*,*,*,*,亞,ア,ア,1/1,C1",
                "　,記号,空白,*,*,*,*,　,　,　,*/*,*",
                "亞,名詞,一般,*,*,*,*,*",
            ]),
        )
        .unwrap();
        assert_eq!("亞 亜", actual);
    }

    #[rstest]
    fn falls_back_to_unknown_character_set() {
        // 1文字ずつの正規化と全体の正規化が食い違う場合は、未知語に含まれる字の集合で置き換える
        let normalize = |s: &str| Ok(s.replace("ab", "Ｘ"));
        let actual = super::normalize_unknown_itaiji(
            "ab亞亞",
            normalize,
            analyzed(&[
                "Ｘ,名詞,一般,*,*,*,*,*",
                "亞,名詞,一般,*,*,*,*,亞,ア,ア,1/1,C1",
                "亞,名詞,一般,*,*,*,*,*",
            ]),
        )
        .unwrap();
        assert_eq!("ab亜亜", actual);
    }
}
