// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)

//! pyopenjtalk-plusのテキスト処理フロントエンドのうち、MeCabとNJDの段階で行う規則。
//!
//! `pyopenjtalk.run_frontend`の既定値（`restore_unknown_katakana=True`、
//! `modify_numeral_reading=True`）での`OpenJTalk.run_frontend`と、その前段の
//! `normalize_unknown_itaiji`、`normalize_roman_numerals`（v0.4.1-post10）に相当する。[`apply_postprocessing`]はpyopenjtalk-plusの
//! 同名の後処理のうち、モデルや外部辞書を使わない規則に相当する。
//!
//! ここにある関数はすべて特徴量の列に対する純粋な関数で、Open JTalkの呼び出しは
//! [`super::open_jtalk`]が担う。

mod accent;
mod before_chaining;
mod context_reading;
mod devoicing;
mod itaiji;
mod itaiji_map;
mod kana;
mod kanji_yomi;
mod kaomoji;
mod known_symbols;
mod loanword_kana;
mod mecab_features;
mod mecab_morphs;
mod number_boundary;
mod odori;
mod reading;
mod roman_numerals;
mod unknown_kanji;
mod unknown_katakana;

use open_jtalk::NjdFeature;

pub(super) use self::{
    itaiji::normalize_unknown_itaiji,
    kaomoji::{
        SilencedKaomoji, find_kaomoji_parts, mark_kaomoji_silent, restore_kaomoji_symbols,
        silence_kaomoji,
    },
    mecab_features::{apply_mecab_rules, expand_unknown_numeral_chunks},
    number_boundary::remove_number_boundaries,
    roman_numerals::normalize_roman_numerals,
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

/// 後処理がOpen JTalkに頼む処理。
pub(super) trait NjdStages {
    /// テキストをMeCabとNJDの全段階で解析し直す。pyopenjtalk-plusの`OpenJTalk.run_frontend`に
    /// 相当し、踊り字の展開で漢字を解析し直すのに使う。
    fn reanalyze(&mut self, text: &str) -> anyhow::Result<Vec<NjdFeature>>;

    /// `njd_set_unvoiced_vowel`だけを通す。pyopenjtalk-plusの`OpenJTalk.set_unvoiced_vowel`。
    fn set_unvoiced_vowel(&mut self, features: &[NjdFeature]) -> Vec<NjdFeature>;

    /// MeCabの特徴量の列をNJDの全段階で解析し、先頭のアクセント核を返す。pyopenjtalk-plusの
    /// `OpenJTalk.run_njd_from_mecab`。
    fn phrase_accent(&mut self, mecab_features: &[String]) -> anyhow::Result<i32>;
}

/// NJDの全段階（`njd_set_long_vowel`まで）の後に適用する後処理。
///
/// pyopenjtalk-plusの`apply_postprocessing`を既定値（`use_vanilla=False`など）で呼んだものに
/// 相当するが、次の処理は行わない。
///
/// - ONNXモデルによる「何」の読みの推定（`predict_nani_reading`）
/// - Sudachiによる読みの補正（`modify_kanji_yomi`は規則で近似する。`read_unknown_kanji`の語単位の
///   読みは行わない）
/// - marineによるアクセントの推定
/// - ユーザー辞書の読み保護
pub(super) fn apply_postprocessing(
    features: Vec<NjdFeature>,
    stages: &mut dyn NjdStages,
) -> anyhow::Result<Vec<NjdFeature>> {
    // 後処理の規則が読みや発音を書き換えた形態素を見分けられるよう、規則を当てる前の読みと発音を控える
    let readings_before_rules = devoicing::readings(&features);
    // フィラーのアクセントは読み変更より先に補正する
    let features = accent::modify_filler_accent(features);
    // pyopenjtalk-plusでSudachiによる読みの上書き（`modify_kanji_yomi`）が担う位置
    let features = kanji_yomi::modify_kanji_yomi(features, &mut |mecab_features| {
        stages.phrase_accent(mecab_features)
    })?;
    let features = reading::suppress_unnatural_auxiliary_u_long_vowel(features);
    let features = context_reading::modify_context_reading(features);
    let features = reading::modify_old_province_yomi(features);
    let features = loanword_kana::restore_loanword_kana(features);
    let features = unknown_kanji::read_unknown_kanji(features);
    // ここまでの後処理は形態素の数を変えないので、位置ごとに規則の前後の読みを比べ、読みか発音を
    // 書き換えた形態素の無声化を書き換えた後の発音で判定し直す
    let features = devoicing::redo_devoicing(&readings_before_rules, features, &mut |features| {
        stages.set_unvoiced_vowel(features)
    });
    let features = accent::retreat_acc_nuc(features);
    let features = accent::modify_acc_after_chaining(features);
    odori::process_odori_features(features, &mut |text| stages.reanalyze(text))
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

    /// pyopenjtalkの`run_frontend`のダンプと同じ順に空白区切りで並べた項目からNJDノードを作る。
    ///
    /// 順序は`string pos pos_group1 ctype cform orig read pron acc mora_size chain_rule
    /// chain_flag`。`pos_group2`と`pos_group3`は`*`。
    pub(super) fn parse_node(line: &str) -> NjdFeature {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let [
            string,
            pos,
            pos_group1,
            ctype,
            cform,
            orig,
            read,
            pron,
            acc,
            mora_size,
            chain_rule,
            chain_flag,
        ] = *fields
        else {
            panic!("expected 12 fields: {line}");
        };
        NjdFeature {
            string: string.to_owned(),
            pos: pos.to_owned(),
            pos_group1: pos_group1.to_owned(),
            pos_group2: "*".to_owned(),
            pos_group3: "*".to_owned(),
            ctype: ctype.to_owned(),
            cform: cform.to_owned(),
            orig: orig.to_owned(),
            read: read.to_owned(),
            pron: pron.to_owned(),
            acc: acc.parse().unwrap(),
            mora_size: mora_size.parse().unwrap(),
            chain_rule: chain_rule.to_owned(),
            chain_flag: chain_flag.parse().unwrap(),
        }
    }

    /// 改行区切りの[`parse_node`]。
    pub(super) fn parse_nodes(lines: &str) -> Vec<NjdFeature> {
        lines
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(parse_node)
            .collect()
    }
}
