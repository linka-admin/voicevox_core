// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/openjtalk.pyx`: `_restore_unknown_katakana_features`)

//! `njd_set_pronunciation`がフィラーへ変えた未知のカタカナ語を、MeCabの品詞へ戻す。

use open_jtalk::NjdFeature;

use std::collections::{HashMap, VecDeque};

use super::kana::is_katakana_word;

/// 読みを持たない未知語（特徴量が12列未満）だけを対象にするため、辞書に登録された本来のフィラーは
/// 維持する。
pub(super) fn restore_unknown_katakana_features(
    mut features: Vec<NjdFeature>,
    mecab_features: &[String],
) -> Vec<NjdFeature> {
    let mut unknown_pos_by_surface = HashMap::<&str, VecDeque<[&str; 4]>>::new();
    for mecab_feature in mecab_features {
        let fields = mecab_feature.split(',').collect::<Vec<_>>();
        if fields.len() < 5 || fields.len() >= 12 {
            continue;
        }
        unknown_pos_by_surface
            .entry(fields[0])
            .or_default()
            .push_back([fields[1], fields[2], fields[3], fields[4]]);
    }

    for feature in &mut features {
        // 英字未知語のフィラー化は英語読み補正が利用するため、全カタカナの表層形へ限定する
        if feature.pos != "フィラー" || !is_katakana_word(&feature.string) {
            continue;
        }
        // 同じ表層形が複数回現れても、MeCabの出現順に対応させる
        let Some([pos, pos_group1, pos_group2, pos_group3]) = unknown_pos_by_surface
            .get_mut(&*feature.string)
            .and_then(VecDeque::pop_front)
        else {
            continue;
        };
        feature.pos = pos.to_owned();
        feature.pos_group1 = pos_group1.to_owned();
        feature.pos_group2 = pos_group2.to_owned();
        feature.pos_group3 = pos_group3.to_owned();
        feature.acc = loanword_accent(&feature.pron);
    }
    features
}

/// 外来語の核を後ろから3モーラ目へ置き、特殊拍に当たる場合は1つ前へ移す。
fn loanword_accent(pron: &str) -> i32 {
    let mut moras = Vec::<String>::new();
    for c in pron.chars() {
        match moras.last_mut() {
            Some(last) if "ャュョァィゥェォ".contains(c) => last.push(c),
            _ => moras.push(c.to_string()),
        }
    }
    if moras.len() <= 3 {
        return 1;
    }
    let mut accent_index = moras.len() - 3;
    while accent_index > 0 && ["ー", "ン", "ッ"].contains(&&*moras[accent_index]) {
        accent_index -= 1;
    }
    (accent_index + 1) as _
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;
    use rstest::rstest;

    use super::super::test_util::node;

    fn filler(surface: &str) -> NjdFeature {
        node(surface, "フィラー", "*", surface)
    }

    fn restore(features: Vec<NjdFeature>, mecab: &[&str]) -> Vec<NjdFeature> {
        let mecab = mecab.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        super::restore_unknown_katakana_features(features, &mecab)
    }

    #[rstest]
    // 外来語の核は後ろから3モーラ目
    #[case("ヌメロワール", 4)]
    // 3モーラ以下は頭高
    #[case("ヌメロ", 1)]
    #[case("ア", 1)]
    // 拗音は前のモーラにまとめる
    #[case("キャンプファイヤー", 5)]
    // 核が特殊拍に当たる場合は前へ移す
    #[case("アカンーッテ", 2)]
    #[case("アンーッテ", 1)]
    fn restores_pos_and_accent(#[case] surface: &str, #[case] expected_acc: i32) {
        let actual = restore(
            vec![filler(surface)],
            &[&format!("{surface},名詞,固有名詞,一般,*,*,*,*")],
        );
        assert_eq!(
            ("名詞", "固有名詞", "一般", "*", expected_acc),
            (
                &*actual[0].pos,
                &*actual[0].pos_group1,
                &*actual[0].pos_group2,
                &*actual[0].pos_group3,
                actual[0].acc,
            ),
        );
    }

    #[rstest]
    fn keeps_dictionary_filler() {
        // 辞書に登録されたフィラー（12列）は対象外
        let actual = restore(
            vec![filler("エート")],
            &["エート,フィラー,*,*,*,*,*,エート,エート,エート,0/3,C1"],
        );
        assert_eq!("フィラー", actual[0].pos);
    }

    #[rstest]
    fn keeps_non_katakana_filler() {
        // 英字未知語のフィラーは英語読み補正が利用するので戻さない
        let actual = restore(vec![filler("ABC")], &["ABC,名詞,一般,*,*,*,*,*"]);
        assert_eq!("フィラー", actual[0].pos);
    }

    #[rstest]
    fn keeps_non_filler() {
        let mut feature = node("ヌメロ", "名詞", "一般", "ヌメロ");
        feature.acc = 0;
        let actual = restore(vec![feature.clone()], &["ヌメロ,名詞,固有名詞,*,*,*,*,*"]);
        assert_eq!(vec![feature], actual);
    }

    #[rstest]
    fn same_surface_follows_mecab_order() {
        let actual = restore(
            vec![
                filler("クールフェーラック"),
                node("と", "助詞", "並立助詞", "ト"),
                filler("クールフェーラック"),
            ],
            &[
                "クールフェーラック,名詞,一般,*,*,*,*,*",
                "と,助詞,並立助詞,*,*,*,*,と,ト,ト,0/1,*",
                "クールフェーラック,名詞,固有名詞,人名,*,*,*,*",
            ],
        );
        assert_eq!(
            [("一般", "*"), ("固有名詞", "人名")],
            [
                (&*actual[0].pos_group1, &*actual[0].pos_group2),
                (&*actual[2].pos_group1, &*actual[2].pos_group2),
            ],
        );
    }
}
