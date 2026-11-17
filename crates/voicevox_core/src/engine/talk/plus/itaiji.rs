// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/utils.py`: `normalize_itaiji`, `normalize_unknown_itaiji`)

//! 辞書で読めない異体字だけを通用字へ置き換える。

use std::collections::{BTreeMap, HashSet};

use super::{
    itaiji_map::tsuyoji,
    mecab_morphs::{Morph, mecab_index_by_original_index, morphs},
    unknown_kanji::is_kanji,
};

/// 辞書で読めない異体字を通用字へ置き換える。
///
/// - `normalize_for_mecab`: Open JTalkの`text2mecab`による正規化。
/// - `analyze`: 入力をMeCabで解析し、特徴量文字列（`表層形,品詞,...`、空白も含む）を返す。
///   異体字を含まない入力では呼ばれない。異体字を含む入力では、最大3回呼ばれる。
///
/// 置き換えるのは、置き換えた字が2字以上のシステム辞書の語に入るか、前後に漢字がなくその字だけで
/// 1語になる場合に限る。「齒性」の「齒」を「歯」にすると1字の語「歯」の「ハ」で読まれてしまうので、
/// 置き換えずに字ごとの音読みの「シ」で読む。
///
/// 辞書（ユーザー辞書を含む）で読める字形は、その字形に固有の読みを残すため置き換えない。同じ異体字
/// が既知語と未知語の両方に現れることがあるので、置き換えるのは未知語の位置にある字だけにする。
pub(crate) fn normalize_unknown_itaiji(
    text: &str,
    normalize_for_mecab: impl Fn(&str) -> anyhow::Result<String>,
    mut analyze: impl FnMut(&str) -> anyhow::Result<Vec<String>>,
) -> anyhow::Result<String> {
    // 異体字を含まない通常の文では、追加の形態素解析をしない
    if !text.chars().any(|c| tsuyoji(c).is_some()) {
        return Ok(text.to_owned());
    }

    let original_features = analyze(text)?;
    // 未知語の位置はMeCab向けに正規化した本文上の位置なので、入力の1文字ずつを正規化して入力上の
    // 位置へ対応付ける。対応付けられない場合は字形を保ち、同じ異体字を含む登録語まで一括で置き換えない
    let Some(mecab_indices) = mecab_index_by_original_index(text, &normalize_for_mecab)? else {
        return Ok(text.to_owned());
    };
    let Some(original_morphs) = morphs(&original_features, &normalize_for_mecab(text)?) else {
        return Ok(text.to_owned());
    };
    let unknown_mecab_indices = original_morphs
        .iter()
        .filter(|morph| morph.is_unknown)
        .flat_map(|morph| morph.span.clone())
        .collect::<HashSet<_>>();
    let characters = text.chars().collect::<Vec<_>>();
    let mut replacements = characters
        .iter()
        .enumerate()
        .filter(|&(index, _)| unknown_mecab_indices.contains(&mecab_indices[index]))
        .filter_map(|(index, &c)| Some((index, tsuyoji(c)?)))
        .collect::<BTreeMap<_, _>>();

    // 置き換えを1つ取り消すと、ほかの字の解析も変わるので、採らない置き換えを外して1回だけ解析し直す
    for _ in 0..2 {
        if replacements.is_empty() {
            break;
        }
        let candidate_text = characters
            .iter()
            .enumerate()
            .map(|(index, c)| *replacements.get(&index).unwrap_or(c))
            .collect::<String>();
        // 通用字の正規化後の長さが異体字と違うと位置を対応付けられないので、置き換えない
        if mecab_index_by_original_index(&candidate_text, &normalize_for_mecab)?.as_ref()
            != Some(&mecab_indices)
        {
            break;
        }
        let candidate_features = analyze(&candidate_text)?;
        let Some(candidate_morphs) =
            morphs(&candidate_features, &normalize_for_mecab(&candidate_text)?)
        else {
            break;
        };
        let retained = replacements
            .iter()
            .filter(|&(&index, _)| {
                is_resolved_replacement(
                    &characters,
                    index,
                    &original_morphs,
                    &candidate_morphs,
                    &mecab_indices,
                )
            })
            .map(|(&index, &proper)| (index, proper))
            .collect::<BTreeMap<_, _>>();
        if retained.len() == replacements.len() {
            return Ok(candidate_text);
        }
        replacements = retained;
    }
    Ok(text.to_owned())
}

/// 置き換えた字が、置き換え後の解析で辞書の語として読めるか。
fn is_resolved_replacement(
    characters: &[char],
    index: usize,
    original_morphs: &[Morph<'_>],
    candidate_morphs: &[Morph<'_>],
    mecab_indices: &[usize],
) -> bool {
    let mecab_index = mecab_indices[index];
    let Some(morph) = candidate_morphs
        .iter()
        .find(|morph| morph.span.contains(&mecab_index))
    else {
        return false;
    };
    if morph.is_unknown {
        return false;
    }
    let span = &morph.span;
    // 「𠮷」「﨔の木」のように前後に漢字がなく、置き換えた字だけで1語になるときは、その語の読みを使う
    if span.len() == 1 {
        return (index == 0 || !is_kanji(characters[index - 1]))
            && (index + 1 == characters.len() || !is_kanji(characters[index + 1]));
    }
    // 2字以上の語は、システム辞書の語のときだけ採る
    if morph.is_user_dictionary {
        return false;
    }
    // 「高 + 﨑駅」を「高﨑 + 駅」と区切り直すように、未知語は区切り直してよい
    // 置き換える前に既知だった語の途中で分ける解析と、ユーザー辞書の語を吸収する解析は採らない
    original_morphs
        .iter()
        .filter(|original| original.span.end > span.start && original.span.start < span.end)
        .filter(|original| !original.is_unknown)
        .all(|original| {
            !original.is_user_dictionary
                && original.span.start >= span.start
                && original.span.end <= span.end
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;

    fn identity(s: &str) -> anyhow::Result<String> {
        Ok(s.to_owned())
    }

    /// 入力ごとに決めた特徴量を返す解析。
    fn analyzer<'a>(
        analyses: &'a [(&'a str, &'a [&'a str])],
    ) -> impl FnMut(&str) -> anyhow::Result<Vec<String>> + 'a {
        let analyses = analyses.iter().copied().collect::<HashMap<_, _>>();
        move |text| {
            let features = analyses
                .get(text)
                .unwrap_or_else(|| panic!("unexpected analysis: {text:?}"));
            Ok(features.iter().map(|&s| s.to_owned()).collect())
        }
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
    // 置き換えた字が2字以上の語に入る
    #[case(
        "𠮷野家",
        &[
            ("𠮷野家", &["𠮷,名詞,一般,*,*,*,*,*", "野家,名詞,固有名詞,*,*,*,*,野家,ノヤ,ノヤ,1/2,C1"][..]),
            ("吉野家", &["吉野家,名詞,固有名詞,組織,*,*,*,吉野家,ヨシノヤ,ヨシノヤ,2/4,C1"][..]),
        ],
        "吉野家"
    )]
    // 前後に漢字がなく、置き換えた字だけで1語になる
    #[case(
        "﨔の木",
        &[
            ("﨔の木", &["﨔,名詞,一般,*,*,*,*,*", "の,助詞,連体化,*,*,*,*,の,ノ,ノ,0/1,*", "木,名詞,一般,*,*,*,*,木,キ,キ,1/1,C3"][..]),
            ("欅の木", &["欅,名詞,一般,*,*,*,*,欅,ケヤキ,ケヤキ,0/3,C2", "の,助詞,連体化,*,*,*,*,の,ノ,ノ,0/1,*", "木,名詞,一般,*,*,*,*,木,キ,キ,1/1,C3"][..]),
        ],
        "欅の木"
    )]
    // 漢字と並ぶ1字の語になるだけなら置き換えない
    #[case(
        "齒性",
        &[
            ("齒性", &["齒,名詞,一般,*,*,*,*,*", "性,名詞,接尾,一般,*,*,*,性,セイ,セー,1/2,C3"][..]),
            ("歯性", &["歯,名詞,一般,*,*,*,*,歯,ハ,ハ,1/1,C3", "性,名詞,接尾,一般,*,*,*,性,セイ,セー,1/2,C3"][..]),
        ],
        "齒性"
    )]
    // 未知語は区切り直してよい
    #[case(
        "高﨑駅",
        &[
            ("高﨑駅", &["高,名詞,一般,*,*,*,*,*", "﨑,名詞,一般,*,*,*,*,*", "駅,名詞,一般,*,*,*,*,駅,エキ,エキ,1/2,C3"][..]),
            ("高崎駅", &["高崎,名詞,固有名詞,地域,一般,*,*,高崎,タカサキ,タカサキ,2/4,C2", "駅,名詞,一般,*,*,*,*,駅,エキ,エキ,1/2,C3"][..]),
        ],
        "高崎駅"
    )]
    // 既知だった語の途中で分ける解析は採らない
    #[case(
        "山田﨑",
        &[
            ("山田﨑", &["山田,名詞,固有名詞,人名,姓,*,*,山田,ヤマダ,ヤマダ,0/3,C2", "﨑,名詞,一般,*,*,*,*,*"][..]),
            ("山田崎", &["山,名詞,一般,*,*,*,*,山,ヤマ,ヤマ,2/2,C3", "田崎,名詞,固有名詞,人名,姓,*,*,田崎,タザキ,タザキ,0/3,C2"][..]),
        ],
        "山田﨑"
    )]
    // 置き換え後の語がユーザー辞書の語なら採らない
    #[case(
        "﨑山",
        &[
            ("﨑山", &["﨑,名詞,一般,*,*,*,*,*", "山,名詞,一般,*,*,*,*,山,ヤマ,ヤマ,2/2,C3"][..]),
            ("崎山", &["崎山,名詞,固有名詞,一般,*,*,*,*,テスト,テスト,1/3,*"][..]),
        ],
        "﨑山"
    )]
    // 辞書で読める字形は残す
    #[case(
        "亞細亞",
        &[("亞細亞", &["亞細亞,名詞,固有名詞,地域,国,*,*,亞細亞,アジア,アジア,1/3,C1"][..])],
        "亞細亞"
    )]
    // 同じ異体字でも、未知語の位置にあるものだけを置き換える
    #[case(
        "﨑山さん。﨑",
        &[
            ("﨑山さん。﨑", &["﨑山,名詞,固有名詞,一般,*,*,*,*,テスト,テスト,1/3,*", "さん,名詞,接尾,人名,*,*,*,さん,サン,サン,0/2,C3", "。,記号,句点,*,*,*,*,。,。,。,0/0,*", "﨑,名詞,一般,*,*,*,*,*"][..]),
            ("﨑山さん。崎", &["﨑山,名詞,固有名詞,一般,*,*,*,*,テスト,テスト,1/3,*", "さん,名詞,接尾,人名,*,*,*,さん,サン,サン,0/2,C3", "。,記号,句点,*,*,*,*,。,。,。,0/0,*", "崎,名詞,一般,*,*,*,*,崎,サキ,サキ,0/2,C3"][..]),
        ],
        "﨑山さん。崎"
    )]
    // 採らない置き換えを外して、1回だけ解析し直す
    #[case(
        "齒性の﨔",
        &[
            ("齒性の﨔", &["齒,名詞,一般,*,*,*,*,*", "性,名詞,接尾,一般,*,*,*,性,セイ,セー,1/2,C3", "の,助詞,連体化,*,*,*,*,の,ノ,ノ,0/1,*", "﨔,名詞,一般,*,*,*,*,*"][..]),
            ("歯性の欅", &["歯,名詞,一般,*,*,*,*,歯,ハ,ハ,1/1,C3", "性,名詞,接尾,一般,*,*,*,性,セイ,セー,1/2,C3", "の,助詞,連体化,*,*,*,*,の,ノ,ノ,0/1,*", "欅,名詞,一般,*,*,*,*,欅,ケヤキ,ケヤキ,0/3,C2"][..]),
            ("齒性の欅", &["齒,名詞,一般,*,*,*,*,*", "性,名詞,接尾,一般,*,*,*,性,セイ,セー,1/2,C3", "の,助詞,連体化,*,*,*,*,の,ノ,ノ,0/1,*", "欅,名詞,一般,*,*,*,*,欅,ケヤキ,ケヤキ,0/3,C2"][..]),
        ],
        "齒性の欅"
    )]
    fn normalize_unknown_itaiji_works(
        #[case] text: &str,
        #[case] analyses: &[(&str, &[&str])],
        #[case] expected: &str,
    ) {
        let actual = super::normalize_unknown_itaiji(text, identity, analyzer(analyses)).unwrap();
        assert_eq!(expected, actual);
    }

    #[rstest]
    fn maps_positions_through_normalization() {
        // 半角空白が全角になってもよい（1文字ずつの正規化と全体の正規化が一致する）
        let normalize = |s: &str| Ok(s.replace(' ', "　"));
        let actual = super::normalize_unknown_itaiji(
            "𠮷 𠮷",
            normalize,
            analyzer(&[
                (
                    "𠮷 𠮷",
                    &[
                        "𠮷,名詞,一般,*,*,*,*,*",
                        "　,記号,空白,*,*,*,*,　,　,　,*/*,*",
                        "𠮷,名詞,一般,*,*,*,*,*",
                    ],
                ),
                (
                    "吉 吉",
                    &[
                        "吉,名詞,固有名詞,人名,名,*,*,吉,ヨシ,ヨシ,1/2,C2",
                        "　,記号,空白,*,*,*,*,　,　,　,*/*,*",
                        "吉,名詞,固有名詞,人名,名,*,*,吉,ヨシ,ヨシ,1/2,C2",
                    ],
                ),
            ]),
        )
        .unwrap();
        assert_eq!("吉 吉", actual);
    }

    #[rstest]
    fn keeps_text_when_positions_cannot_be_mapped() {
        // 1文字ずつの正規化と全体の正規化が食い違う場合は、字形を保つ
        let normalize = |s: &str| Ok(s.replace("ab", "Ｘ"));
        let actual = super::normalize_unknown_itaiji(
            "ab𠮷",
            normalize,
            analyzer(&[(
                "ab𠮷",
                &["Ｘ,名詞,一般,*,*,*,*,*", "𠮷,名詞,一般,*,*,*,*,*"][..],
            )]),
        )
        .unwrap();
        assert_eq!("ab𠮷", actual);
    }
}
