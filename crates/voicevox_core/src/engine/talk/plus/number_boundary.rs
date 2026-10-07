// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/openjtalk.pyx`: `OpenJTalk._run_njd_from_mecab`の数詞の境界ノードの処理)

//! 「EF65 1032号機」の「65」と「1032」を別々に位取りするための、一時的な境界ノード。
//!
//! `njd_set_digit`の前に[`insert_number_boundaries`]で挿入し、後に[`remove_number_boundaries`]で
//! 取り除く。

use open_jtalk::NjdFeature;

/// 境界ノードの`pos_group3`と`pron`。`pron`が`*`だと数詞処理の中で消えるため別の値を使う。
const BOUNDARY: &str = "空白境界";

/// 結合境界（`chain_flag == 0`）を持つ数詞が数詞の後に続く箇所に、境界ノードを挿入する。
///
/// 挿入したかどうかも返す。
pub(super) fn insert_number_boundaries(features: Vec<NjdFeature>) -> (Vec<NjdFeature>, bool) {
    let mut bounded = Vec::with_capacity(features.len());
    let mut inserted = false;
    for (index, current) in features.iter().enumerate() {
        if index > 0
            && current.pos_group1 == "数"
            && features[index - 1].pos_group1 == "数"
            && current.chain_flag == 0
        {
            bounded.push(NjdFeature {
                string: "".to_owned(),
                orig: "".to_owned(),
                read: "*".to_owned(),
                pron: BOUNDARY.to_owned(),
                pos_group1: "一般".to_owned(),
                pos_group3: BOUNDARY.to_owned(),
                acc: 0,
                mora_size: 0,
                chain_rule: "*".to_owned(),
                ..current.clone()
            });
            inserted = true;
        }
        bounded.push(current.clone());
    }
    (bounded, inserted)
}

/// 境界ノードを取り除き、その直後のノードを結合境界にする。
pub(crate) fn remove_number_boundaries(features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    let mut result = Vec::with_capacity(features.len());
    let mut after_boundary = false;
    for mut current in features {
        if current.pos_group3 == BOUNDARY {
            after_boundary = true;
            continue;
        }
        if after_boundary {
            current.chain_flag = 0;
            after_boundary = false;
        }
        result.push(current);
    }
    result
}

#[cfg(test)]
mod tests {
    use open_jtalk::NjdFeature;
    use rstest::rstest;

    use super::super::test_util::node;

    fn number(string: &str, chain_flag: i32) -> NjdFeature {
        let mut f = node(string, "名詞", "数", "イチ");
        f.acc = 2;
        f.mora_size = 2;
        f.chain_rule = "C3".to_owned();
        f.chain_flag = chain_flag;
        f
    }

    fn boundary_of(f: &NjdFeature) -> NjdFeature {
        NjdFeature {
            string: "".to_owned(),
            orig: "".to_owned(),
            read: "*".to_owned(),
            pron: "空白境界".to_owned(),
            pos_group1: "一般".to_owned(),
            pos_group3: "空白境界".to_owned(),
            acc: 0,
            mora_size: 0,
            chain_rule: "*".to_owned(),
            ..f.clone()
        }
    }

    #[rstest]
    fn inserts_boundary_between_numbers() {
        let features = vec![number("５", -1), number("１", 0), number("０", -1)];
        let (actual, inserted) = super::insert_number_boundaries(features.clone());
        assert!(inserted);
        assert_eq!(
            vec![
                features[0].clone(),
                boundary_of(&features[1]),
                features[1].clone(),
                features[2].clone(),
            ],
            actual,
        );
    }

    #[rstest]
    #[case(vec![number("１", 0), number("０", -1)])]
    #[case(vec![node("猫", "名詞", "一般", "ネコ"), number("１", 0)])]
    #[case(vec![number("５", -1), number("１", 1)])]
    fn does_not_insert_otherwise(#[case] features: Vec<NjdFeature>) {
        let (actual, inserted) = super::insert_number_boundaries(features.clone());
        assert!(!inserted);
        assert_eq!(features, actual);
    }

    #[rstest]
    fn removes_boundary_and_breaks_chain() {
        let five = number("五", 1);
        let one = number("千", 1);
        let features = vec![
            five.clone(),
            boundary_of(&one),
            one.clone(),
            number("二", 1),
        ];
        let actual = super::remove_number_boundaries(features);
        assert_eq!(
            vec![
                five,
                NjdFeature {
                    chain_flag: 0,
                    ..one
                },
                number("二", 1)
            ],
            actual,
        );
    }
}
