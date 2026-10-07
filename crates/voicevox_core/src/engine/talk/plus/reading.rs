// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/utils.py`: `suppress_unnatural_auxiliary_u_long_vowel`,
// `modify_old_province_yomi`)

//! 隣の形態素だけで決まる読みの後処理。

use open_jtalk::NjdFeature;

use super::kana::{Dan, dan};

/// 直後に接尾辞「国」が続くと、「国」を「コク」でなく「ノクニ」と読む令制国の名前
/// （例: 石見国 = イワミノクニ）。
///
/// 「中国」「外国」のようにMeCabが1語として解析する語は、接尾辞の条件で対象から外れる。
const OLD_PROVINCE_NAMES: &[&str] = &[
    "山城",
    "大和",
    "河内",
    "和泉",
    "摂津",
    "伊賀",
    "伊勢",
    "志摩",
    "尾張",
    "三河",
    "遠江",
    "駿河",
    "伊豆",
    "甲斐",
    "相模",
    "武蔵",
    "安房",
    "上総",
    "下総",
    "常陸",
    "近江",
    "美濃",
    "飛騨",
    "信濃",
    "上野",
    "下野",
    "陸奥",
    "出羽",
    "若狭",
    "越前",
    "加賀",
    "能登",
    "越中",
    "越後",
    "佐渡",
    "丹波",
    "丹後",
    "但馬",
    "因幡",
    "伯耆",
    "出雲",
    "石見",
    "隠岐",
    "播磨",
    "美作",
    "備前",
    "備中",
    "備後",
    "安芸",
    "周防",
    "長門",
    "紀伊",
    "淡路",
    "阿波",
    "讃岐",
    "伊予",
    "土佐",
    "筑前",
    "筑後",
    "豊前",
    "豊後",
    "肥前",
    "肥後",
    "日向",
    "大隅",
    "薩摩",
    "壱岐",
    "対馬",
    "岩代",
    "磐城",
    "陸前",
    "陸中",
    "羽前",
    "羽後",
    "渡島",
    "後志",
    "胆振",
    "石狩",
    "天塩",
    "北見",
    "日高",
    "十勝",
    "釧路",
    "根室",
    "千島",
    "大倭",
    "御野",
    "諏方",
    "石城",
    "石背",
    "多禰",
    "筑紫",
    "末廬",
    "高志",
    "上毛野",
    "下毛野",
    "三野",
    "針間",
    "吉備",
    "科野",
];

/// 動詞・助動詞に続く助動詞「う」の長音化のうち、直前の語末がア段・イ段・エ段のものを「ウ」に戻す。
///
/// ref: <https://github.com/tsukumijima/pyopenjtalk-plus/issues/6#issuecomment-4067840409>
pub(super) fn suppress_unnatural_auxiliary_u_long_vowel(
    mut features: Vec<NjdFeature>,
) -> Vec<NjdFeature> {
    for i in 1..features.len() {
        let (current, next) = (&features[i - 1], &features[i]);
        if next.pron != "ー" || next.read != "ウ" {
            continue;
        }
        let previous_dan = current
            .pron
            .trim_end_matches('’')
            .chars()
            .last()
            .and_then(dan);
        if matches!(previous_dan, Some(Dan::A | Dan::I | Dan::E)) {
            features[i].pron = "ウ".to_owned();
        }
    }
    features
}

/// 令制国の名前に続く接尾辞「国」の読みを「コク」から「ノクニ」に変える。
///
/// 名前の側の読みは変えないので、名前そのものを誤読する語は辞書で直す必要がある。
pub(super) fn modify_old_province_yomi(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    for i in 1..features.len() {
        let (previous, feature) = (&features[i - 1], &features[i]);
        if feature.string != "国"
            || feature.pos_group1 != "接尾"
            || !OLD_PROVINCE_NAMES.contains(&&*previous.string)
        {
            continue;
        }
        let feature = &mut features[i];
        feature.read = "ノクニ".to_owned();
        feature.pron = "ノクニ".to_owned();
        feature.mora_size = 3;
    }
    features
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::test_util::parse_nodes;

    fn prons(features: &[open_jtalk::NjdFeature]) -> Vec<&str> {
        features.iter().map(|f| &*f.pron).collect()
    }

    #[rstest]
    // ア段・イ段・エ段に続く「ー」は「ウ」に戻す
    #[case(
        "買わ 動詞 自立 五段・ワ行促音便 未然ウ接続 買う カワ カワ 0 2 * -1
         う 助動詞 * 不変化型 基本形 う ウ ー 0 1 * 1",
        &["カワ", "ウ"],
    )]
    #[case(
        "捨て 動詞 自立 一段 未然形 捨てる ステ ス’テ 0 2 * -1
         う 助動詞 * 不変化型 基本形 う ウ ー 0 1 * 1",
        &["ス’テ", "ウ"],
    )]
    // オ段に続く長音はそのまま
    #[case(
        "行こ 動詞 自立 五段・カ行促音便 未然ウ接続 行く イコ イコ 0 2 * -1
         う 助動詞 * 不変化型 基本形 う ウ ー 0 1 * 1",
        &["イコ", "ー"],
    )]
    fn suppress_unnatural_auxiliary_u_long_vowel_works(
        #[case] features: &str,
        #[case] expected: &[&str],
    ) {
        let actual = super::suppress_unnatural_auxiliary_u_long_vowel(parse_nodes(features));
        assert_eq!(expected, prons(&actual));
    }

    #[rstest]
    #[case(
        "石見 名詞 固有名詞 * * 石見 イワミ イワミ 0 3 C2 -1
         国 名詞 接尾 * * 国 コク コク 1 2 C3 1",
        &["イワミ", "ノクニ"],
    )]
    // 1語の「中国」や、接尾辞でない「国」は変えない
    #[case(
        "中国 名詞 固有名詞 * * 中国 チュウゴク チューゴク 1 4 C2 -1",
        &["チューゴク"],
    )]
    #[case(
        "石見 名詞 固有名詞 * * 石見 イワミ イワミ 0 3 C2 -1
         国 名詞 一般 * * 国 クニ クニ 1 2 C3 1",
        &["イワミ", "クニ"],
    )]
    fn modify_old_province_yomi_works(#[case] features: &str, #[case] expected: &[&str]) {
        let actual = super::modify_old_province_yomi(parse_nodes(features));
        assert_eq!(expected, prons(&actual));
        if expected.last() == Some(&"ノクニ") {
            let last = actual.last().unwrap();
            assert_eq!(("ノクニ", 3), (&*last.read, last.mora_size));
        }
    }
}
