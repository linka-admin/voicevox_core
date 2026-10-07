// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63

//! pyopenjtalk-plusのテキスト処理フロントエンドのうち、MeCabとNJDの段階で行う規則。
//!
//! `pyopenjtalk.run_frontend`の既定値（`restore_unknown_katakana=True`、
//! `modify_numeral_reading=True`）での`OpenJTalk.run_frontend`と、その前段の
//! `normalize_unknown_itaiji`に相当する。`apply_postprocessing`による後処理は含まない。
//!
//! ここにある関数はすべて特徴量の列に対する純粋な関数で、Open JTalkの呼び出しは
//! [`super::open_jtalk`]が担う。

mod before_chaining;
mod itaiji;
mod itaiji_map;
mod kana;
mod known_symbols;
mod mecab_features;
mod number_boundary;
mod unknown_katakana;

use open_jtalk::NjdFeature;

pub(super) use self::{
    itaiji::normalize_unknown_itaiji,
    mecab_features::{apply_mecab_rules, expand_unknown_numeral_chunks},
    number_boundary::remove_number_boundaries,
};

/// `njd_set_pronunciation`の後、`njd_set_digit`の前に適用する規則。
///
/// `mecab_features`は[`apply_mecab_rules`]を適用した後、[`expand_unknown_numeral_chunks`]を適用
/// する前のもの。
///
/// 数詞の区切りを表す一時的なノードを挿入したかどうかも返す。挿入した場合は`njd_set_digit`の後に
/// [`remove_number_boundaries`]で取り除く必要がある。
pub(super) fn apply_njd_rules_before_digit(
    features: Vec<NjdFeature>,
    mecab_features: &[String],
) -> (Vec<NjdFeature>, bool) {
    let features = unknown_katakana::restore_unknown_katakana_features(features, mecab_features);
    let features = before_chaining::apply_original_rule_before_chaining(features, true);
    number_boundary::insert_number_boundaries(features)
}

#[cfg(test)]
mod test_util {
    use open_jtalk::NjdFeature;

    /// テスト用のNJDノード。指定しない項目は`*`、`chain_flag`は`-1`。
    pub(super) fn node(string: &str, pos: &str, pos_group1: &str, pron: &str) -> NjdFeature {
        NjdFeature {
            string: string.to_owned(),
            pos: pos.to_owned(),
            pos_group1: pos_group1.to_owned(),
            pos_group2: "*".to_owned(),
            pos_group3: "*".to_owned(),
            ctype: "*".to_owned(),
            cform: "*".to_owned(),
            orig: string.to_owned(),
            read: pron.to_owned(),
            pron: pron.to_owned(),
            acc: 0,
            mora_size: 0,
            chain_rule: "*".to_owned(),
            chain_flag: -1,
        }
    }
}
