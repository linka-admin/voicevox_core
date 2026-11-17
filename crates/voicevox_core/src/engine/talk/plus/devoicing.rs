// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/__init__.py`: `apply_postprocessing`の母音の無声化の判定し直し)

//! 後処理で読みか発音を書き換えた形態素の、母音の無声化の判定し直し。
//!
//! NJDは書き換える前の発音で母音の無声化を判定したので、読みか発音を書き換えた形態素とその直前の
//! 形態素の末尾は、書き換えた後の発音で判定し直す。「博士課程」の「ハクシ」は「ク」が無声化し、
//! 「東高洲橋」の「ス」は連濁した「バシ」の前では無声化しない。

use open_jtalk::NjdFeature;

const DEVOICING_MARK: char = '’';

/// 後処理の規則を当てる前の読みと発音。
pub(super) fn readings(features: &[NjdFeature]) -> Vec<(String, String)> {
    features
        .iter()
        .map(|f| (f.read.clone(), f.pron.clone()))
        .collect()
}

/// `before`（[`readings`]）から読みか発音が変わった形態素とその直前の形態素の無声化を、
/// `set_unvoiced_vowel`（`njd_set_unvoiced_vowel`）で付け直す。
///
/// `set_unvoiced_vowel`はすでに付いている`’`を残すので、付け直す箇所の`’`を外してから渡す。
/// 後処理は形態素の数を変えない（`before`と`features`の長さが同じ）ことを前提とする。
pub(super) fn redo_devoicing(
    before: &[(String, String)],
    mut features: Vec<NjdFeature>,
    set_unvoiced_vowel: &mut dyn FnMut(&[NjdFeature]) -> Vec<NjdFeature>,
) -> Vec<NjdFeature> {
    assert_eq!(before.len(), features.len());
    let rewritten = features
        .iter()
        .zip(before)
        .enumerate()
        .filter(|(_, (f, (read, pron)))| (&f.read, &f.pron) != (read, pron))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if rewritten.is_empty() {
        return features;
    }
    for &index in &rewritten {
        features[index].pron.retain(|c| c != DEVOICING_MARK);
        if let Some(previous) = index.checked_sub(1).map(|i| &mut features[i])
            && previous.pron.ends_with(DEVOICING_MARK)
        {
            previous.pron.pop();
        }
    }
    let revoiced = set_unvoiced_vowel(&features);
    assert_eq!(revoiced.len(), features.len());
    for &index in &rewritten {
        for target in index.saturating_sub(1)..=index {
            features[target].pron.clone_from(&revoiced[target].pron);
        }
    }
    features
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;

    use super::super::test_util::parse_nodes;

    /// 「シ」「ク」の後ろが無声子音のとき、その母音を無声化する簡易版の`njd_set_unvoiced_vowel`。
    fn fake_set_unvoiced_vowel(features: &[NjdFeature]) -> Vec<NjdFeature> {
        let prons = features.iter().map(|f| f.pron.clone()).collect::<String>();
        let chars = prons.chars().collect::<Vec<_>>();
        let mut offset = 0;
        features
            .iter()
            .map(|f| {
                let mut f = f.clone();
                let len = f.pron.chars().count();
                let mut pron = String::new();
                for (i, c) in f.pron.chars().enumerate() {
                    pron.push(c);
                    let next = chars.get(offset + i + 1);
                    if "シク".contains(c)
                        && next.is_some_and(|&n| {
                            "カキクケコサシスセソタチツテトハヒフヘホ".contains(n)
                        })
                    {
                        pron.push('’');
                    }
                }
                offset += len;
                f.pron = pron;
                f
            })
            .collect()
    }

    #[test]
    fn it_redoes_devoicing_of_rewritten_features_and_the_previous_one() {
        // 「博士課程」の「ハカセ」を「ハクシ」に書き換えた
        let before = parse_nodes(
            "
            医学 名詞 一般 * * 医学 イガク イガク’ 1 3 C1 -1
            博士 名詞 一般 * * 博士 ハカセ ハカセ 1 3 C1 1
            課程 名詞 一般 * * 課程 カテイ カテー 0 3 C1 1
            ",
        );
        let mut features = before.clone();
        features[1].read = "ハクシ".to_owned();
        features[1].pron = "ハクシ".to_owned();
        let actual = super::redo_devoicing(
            &super::readings(&before),
            features,
            &mut fake_set_unvoiced_vowel,
        );
        assert_eq!(
            ["イガク’", "ハク’シ’", "カテー"],
            [0, 1, 2].map(|i| &*actual[i].pron),
        );
    }

    #[test]
    fn it_keeps_features_without_rewrites() {
        let before = parse_nodes("北 名詞 一般 * * 北 キタ キ’タ 0 2 C1 -1");
        let actual = super::redo_devoicing(&super::readings(&before), before.clone(), &mut |_| {
            panic!("should not be called")
        });
        assert_eq!(before, actual);
    }

    #[test]
    fn it_releases_devoicing_of_previous_feature_when_rewritten() {
        // 「東高洲橋」の「ハシ」を連濁した「バシ」に書き換えると「ス」は無声化しない
        let before = parse_nodes(
            "
            高洲 名詞 固有名詞 * * 高洲 タカス タカス’ 0 3 C1 -1
            橋 名詞 接尾 * * 橋 ハシ ハシ 0 2 C1 1
            ",
        );
        let mut features = before.clone();
        features[1].read = "バシ".to_owned();
        features[1].pron = "バシ".to_owned();
        let actual = super::redo_devoicing(
            &super::readings(&before),
            features,
            &mut fake_set_unvoiced_vowel,
        );
        assert_eq!(["タカス", "バシ"], [0, 1].map(|i| &*actual[i].pron));
    }
}
