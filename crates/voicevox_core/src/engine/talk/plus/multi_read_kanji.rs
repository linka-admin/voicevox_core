// Based on pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/__init__.py`: `MULTI_READ_KANJI_LIST`, `pyopenjtalk/utils.py`: `modify_kanji_yomi`)

//! 読みが複数ある漢字の無声化の解除。
//!
//! pyopenjtalk-plusの`modify_kanji_yomi`は、これらの漢字1文字の形態素の発音をSudachiの読みで
//! 上書きする。その副作用で発音に付いていた無声化記号`’`が消え、無声化されなくなる。実際の
//! 変化の大半はこの無声化の解除で、読みそのものが変わるのはまれであるため、Sudachiは使わずに
//! 無声化の解除だけを再現する。

use open_jtalk::NjdFeature;

/// pyopenjtalk-plusの`MULTI_READ_KANJI_LIST`から「何」を除いたもの。
const MULTI_READ_KANJI: &[char] = &[
    '風', '観', '方', '出', '時', '上', '下', '君', '手', '嫌', '表', '対', '色', '人', '前', '後',
    '角', '金', '頭', '筆', '水', '間', '棚', '床', '入', '来', '塗', '怒', '包', '被', '開', '弾',
    '捻', '潜', '支', '抱', '行', '降', '種', '訳', '糞', '空', '性', '体', '等', '生', '止', '堪',
    '捩', '家', '縁', '労', '中', '高', '低', '気', '要', '退', '面', '主', '術', '直', '片', '緒',
    '小', '大', '値',
];

const DEVOICING_MARK: char = '’';

/// 読みが複数ある漢字1文字の形態素から、無声化記号を取り除く。
///
/// 接尾辞は対象外。「方」を「ホウ」と読む場合も対象外（pyopenjtalk-plusでは長音の発音
/// 「ホー」に置き換えられ、無声化記号はもともと付かない）。
pub(super) fn release_multi_read_kanji_devoicing(features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    features
        .into_iter()
        .map(|mut feature| {
            if is_target(&feature) {
                feature.pron.retain(|c| c != DEVOICING_MARK);
            }
            feature
        })
        .collect()
}

fn is_target(feature: &NjdFeature) -> bool {
    let mut chars = feature.orig.chars();
    let (Some(kanji), None) = (chars.next(), chars.next()) else {
        return false;
    };
    MULTI_READ_KANJI.contains(&kanji)
        && feature.pos_group1 != "接尾"
        && !(kanji == '方' && feature.read == "ホウ")
}

#[cfg(test)]
mod tests {
    use super::{super::test_util::parse_nodes, release_multi_read_kanji_devoicing};

    #[test]
    fn it_releases_devoicing_of_multi_read_kanji() {
        // 人は
        let features = parse_nodes(
            "
            人 名詞 一般 * * 人 ヒト ヒ’ト 0 2 C1 -1
            は 助詞 係助詞 * * は ハ ワ 0 1 * 1
            ",
        );
        let expected = parse_nodes(
            "
            人 名詞 一般 * * 人 ヒト ヒト 0 2 C1 -1
            は 助詞 係助詞 * * は ハ ワ 0 1 * 1
            ",
        );
        assert_eq!(expected, release_multi_read_kanji_devoicing(features));
    }

    #[test]
    fn it_keeps_suffixes_and_other_words() {
        let features = parse_nodes(
            "
            日本 名詞 固有名詞 * * 日本 ニッポン ニッポン 3 4 C1 -1
            人 名詞 接尾 * * 人 ジン ジン 1 2 C3 1
            北 名詞 一般 * * 北 キタ キ’タ 0 2 C1 -1
            人々 名詞 一般 * * 人々 ヒトビト ヒ’トビト 2 4 C1 -1
            ",
        );
        assert_eq!(
            features.clone(),
            release_multi_read_kanji_devoicing(features)
        );
    }

    #[test]
    fn it_keeps_hou_reading_of_kata() {
        let features = parse_nodes("方 名詞 一般 * * 方 ホウ ホ’ウ 0 2 C1 -1");
        assert_eq!(
            features.clone(),
            release_multi_read_kanji_devoicing(features)
        );
    }
}
