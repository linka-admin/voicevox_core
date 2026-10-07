// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ 3310e63
// (`pyopenjtalk/utils.py`: `modify_context_reading`, `_is_negative_nannimo_context`)

//! 前後の形態素で読みが決まる語（「駆け込み寺」の「デラ」、「先生方」の「ガタ」など）の読みを
//! 書き換える。

use open_jtalk::NjdFeature;

use super::kana::mora_count;

/// 「〜の下」を「モト」と読ませる抽象名詞。
const ABSTRACT_NO_PREDECESSORS: &[&str] = &[
    "支配", "統治", "指導", "指揮", "監督", "管理", "監視", "命令", "号令", "庇護", "保護", "援助",
    "協力", "後援", "統制", "占領", "名", "法", "条件", "前提", "仮定", "原則", "方針", "契約",
    "規定", "制度", "計画", "設定", "愛情", "信頼", "理解", "合意", "影響", "配慮", "恩師", "陛下",
    "殿下", "親方",
];

/// 「方」を「ガタ」と読ませる敬称・複数の前接語。
const HONORIFIC_PLURAL_PREDECESSORS: &[&str] = &[
    "皆様",
    "皆",
    "みんな",
    "あなた",
    "先生",
    "奥様",
    "お客様",
    "親御",
    "殿",
];

/// 「前」を「ゼン」と読ませる前接語。
const ZEN_PREDECESSORS: &[&str] = &[
    "紀元", "門", "生", "就学", "出生", "公判", "産", "術", "食", "陸", "膝蓋", "祝典", "患難",
    "停滞", "閉塞", "寒帯", "温暖", "暴露",
];

/// 「前」を「ゼン」と読ませる後続の役職名と段階の語。
const ZEN_SUCCESSORS: &[&str] = &["会長", "大統領", "段階", "理事長", "社長", "総裁", "首相"];

/// 「橋」を「キョー」と読ませる構造種別の前接語。
const BRIDGE_TYPE_PREDECESSORS: &[&str] = &[
    "高架",
    "可動",
    "水管",
    "人道",
    "跨道",
    "跨線",
    "連絡",
    "斜張",
    "張",
    "河口",
    "併用",
    "吊",
    "桁",
    "鉄道",
    "歩道",
    "陸",
    "仮設",
    "道路",
    "段",
    "ＰＣ",
    "アーチ",
    "トラス",
    "ラーメン",
];

/// 「寺」を「デラ」と読ませる前接語。
const DERA_PREDECESSORS: &[&str] = &[
    "縁切",
    "駆け込み",
    "田舎",
    "猫",
    "だるま",
    "隠れ",
    "峯",
    "山",
    "花",
];

/// 「寺」を「ジ」と読ませる実証済みの前接語。
const JI_PREDECESSORS: &[&str] = &["霊山"];

/// 「縁」を「フチ」と読む器物や形状を表す前接語。
const FUCHI_PREDECESSORS: &[&str] = &[
    "カップ",
    "器",
    "堀",
    "崖",
    "径",
    "屏風",
    "内側",
    "口",
    "皿",
    "模様",
    "火鉢",
    "金",
];

/// 「ひがみ入ってます」の名詞用法も動詞として解析されるため、複合動詞の「入る」は確認できた
/// 前接語に限る。
const IRU_COMPOUND_PREDECESSORS: &[&str] = &["走り", "攻め", "折り", "分け"];

/// 「大」は後続する語によって「オー」と「ダイ」が分かれるため、表層形で判定する。
const OO_SUCCESSORS: &[&str] = &[
    "にぎわい",
    "丸髷",
    "地主",
    "旦那",
    "泥坊",
    "津波",
    "掃除",
    "番狂わせ",
    "番頭",
    "違い",
];

/// 「御」は「御言葉」の「オ」と「御住所」の「ゴ」を後続する語で分ける。
const O_SUCCESSORS: &[&str] = &[
    "仕置",
    "屋敷",
    "帰り",
    "急ぎ",
    "手際",
    "支払い",
    "楽しみ",
    "気の毒",
    "田植祭",
    "神籤",
    "粗末",
    "言葉",
    "近く",
    "隣",
    "嬢",
];

/// 「の」を挟んで道具が前に来る「柄」は「エ」と読む。
const TOOL_HANDLE_PREDECESSORS: &[&str] = &[
    "うちわ",
    "やり",
    "傘",
    "剃刀",
    "団扇",
    "提灯",
    "斧",
    "柄杓",
    "槍",
    "洋傘",
    "箒",
    "薙刀",
    "鋏",
    "鋤",
    "鋸",
    "鍬",
    "錫杖",
];

/// 「の」を挟んで刀剣が前に来る「柄」は「ツカ」と読む。
const SWORD_HILT_PREDECESSORS: &[&str] = &[
    "刀",
    "剣",
    "大刀",
    "太刀",
    "小剣",
    "懐剣",
    "木剣",
    "短刀",
    "短剣",
    "脇差",
    "軍刀",
    "長脇差",
    "鎧通し",
];

/// 空間を比較する「より外」は「ソト」なので、「ホカ」への補正は動詞・代名詞と打ち消しの組に限る。
const NEGATIVE_ORIGINALS: &[&str] = &["ない", "無い", "ぬ", "ん", "まい", "ず"];

/// 「何にも知らない」「何にもならない」のように、打ち消しと組んで「ナンニモ」と読む述語。
///
/// 「何にも似ていない」「何にも代えがたい」は格助詞の「に」を保ち、「ナニニモ」と読む。
const NANNIMO_PREDICATES: &[&str] = &[
    "知る",
    "わかる",
    "分かる",
    "分る",
    "する",
    "出来る",
    "なる",
    "言う",
    "やる",
    "食べる",
    "聞く",
    "答える",
    "ある",
    "ない",
    "無い",
    "面白い",
];

/// 名詞直後の後部要素へ与える複合語の読み（表層形、読み、発音、対象の品詞細分類）。
///
/// 品詞細分類が`None`のものは品詞を問わない。
const COMPOUND_SUFFIX_READINGS: &[(&str, &str, &str, Option<&str>)] = &[
    ("不足", "ブソク", "ブソク", Some("サ変接続")),
    ("焼", "ヤキ", "ヤキ", Some("接尾")),
    ("峡", "キョウ", "キョー", Some("一般")),
    ("屯", "トン", "トン", Some("サ変接続")),
    ("角形", "カクケイ", "カクケー", Some("一般")),
    ("通", "ドオリ", "ドーリ", Some("固有名詞")),
    ("旗", "キ", "キ", Some("一般")),
    ("環", "カン", "カン", None),
    ("洞", "ドウ", "ドー", None),
    ("湖", "コ", "コ", None),
    ("唇", "シン", "シン", None),
    ("印", "イン", "イン", None),
    ("塚", "ズカ", "ズカ", None),
    ("小屋", "ゴヤ", "ゴヤ", None),
    ("部屋", "ベヤ", "ベヤ", None),
    ("付", "ツキ", "ツキ", Some("接尾")),
    ("金", "キン", "キン", Some("一般")),
    ("公", "コウ", "コー", Some("一般")),
    ("硬", "コウ", "コー", None),
];

/// 「より外にない」の「ホカ」は、後続する打ち消しの語も確かめてから適用する。
pub(super) fn modify_context_reading(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    for index in 0..features.len() {
        if let Some((reading, pronunciation)) = context_reading(&features, index) {
            set_reading(&mut features, index, reading, pronunciation);
        }
    }
    features
}

/// 読みと発音を書き換え、モーラ数の変更に合わせてアクセント句の核の位置を補正する。
///
/// `pronunciation`が`None`の場合は読みと同じにする。
fn set_reading(
    features: &mut [NjdFeature],
    index: usize,
    reading: &str,
    pronunciation: Option<&str>,
) {
    let new_pronunciation = pronunciation.unwrap_or(reading);
    let feature = &features[index];
    // 読みが合っている場合は、NJDが付けた無声化記号もそのまま使う
    if feature.read == reading && feature.pron.replace('’', "") == new_pronunciation {
        return;
    }

    let old_mora_size = feature.mora_size;
    let old_pronunciation = feature.pron.replace('’', "");
    let feature = &mut features[index];
    feature.read = reading.to_owned();
    feature.pron = new_pronunciation.to_owned();
    feature.mora_size = mora_count(new_pronunciation);
    let (string, mora_size) = (feature.string.clone(), feature.mora_size);

    // NJDが数えた句の核を、変更後も同じ後続モーラに置く
    if mora_size == old_mora_size {
        return;
    }
    let mut head = index;
    while head > 0 && features[head].chain_flag == 1 {
        head -= 1;
    }
    let preceding_mora_size = features[head..index]
        .iter()
        .map(|f| f.mora_size)
        .sum::<i32>();
    let accent = features[head].acc;
    if accent > preceding_mora_size + old_mora_size {
        features[head].acc += mora_size - old_mora_size;
    } else if accent > preceding_mora_size {
        // 短い湖名では、「ミズウミ」の短縮で消える拍の核を前の要素の末尾に置く
        // C1の加算で得た核を「コ」に丸めると尾高型へ変わるので、3モーラ以下の下がり目を保つ
        features[head].acc = if string == "湖"
            && old_pronunciation == "ミズウミ"
            && new_pronunciation == "コ"
            && preceding_mora_size + mora_size <= 3
            && accent > preceding_mora_size + mora_size
        {
            preceding_mora_size
        } else {
            preceding_mora_size + (accent - preceding_mora_size).min(mora_size)
        };
    }
}

/// `index`の形態素に与える読みと発音。
///
/// 条件は上から順に調べ、最初に当てはまった条件で読みを決める（読みを変えない場合も含む）。
fn context_reading(
    features: &[NjdFeature],
    index: usize,
) -> Option<(&'static str, Option<&'static str>)> {
    let feature = &features[index];
    let previous = index.checked_sub(1).map(|i| &features[i]);
    let previous_previous = index.checked_sub(2).map(|i| &features[i]);
    let following = features.get(index + 1);
    let surface = &*feature.string;

    let previous_string_in = |words: &[&str]| previous.is_some_and(|f| words.contains(&&*f.string));
    let previous_previous_string_in =
        |words: &[&str]| previous_previous.is_some_and(|f| words.contains(&&*f.string));
    let following_string_in =
        |words: &[&str]| following.is_some_and(|f| words.contains(&&*f.string));
    let previous_string_is = |word: &str| previous.is_some_and(|f| f.string == word);
    let previous_is_noun_of = |groups: &[&str]| {
        previous.is_some_and(|f| f.pos == "名詞" && groups.contains(&&*f.pos_group1))
    };

    // 「何にも」の副詞の行を優先すると「何にも依存しない」も変わるため、打ち消しの述語で読みを選ぶ
    if surface == "何"
        && feature.pos == "名詞"
        && index + 2 < features.len()
        && features[index + 1].string == "に"
        && features[index + 1].pos_group1 == "格助詞"
        && features[index + 2].string == "も"
        && features[index + 2].pos_group1 == "係助詞"
        && is_negative_nannimo_context(features, index + 3)
    {
        return Some(("ナン", None));
    }
    // 直後の語で意味が確定する少数の表現を、閉じた表層形の集合で判定する
    if surface == "一見" && following_string_in(&["さん"]) {
        return Some(("イチゲン", None));
    }
    if surface == "一声" && following_string_in(&["かけ", "掛け", "かける", "掛ける"]) {
        return Some(("ヒトコエ", None));
    }
    if surface == "一行" && following_string_in(&["ごと", "毎"]) {
        return Some(("イチギョウ", Some("イチギョー")));
    }
    if surface == "町" && following_string_in(&["史"]) {
        return Some(("チョウ", Some("チョー")));
    }
    if surface == "一" && following_string_in(&["しょ"]) {
        return Some(("イッ", None));
    }
    // 「返戻金型」は「返戻金」に「型」が付く表現なので、鋳型の「金型」と分けて「キン」を保つ
    if surface == "金" && following_string_in(&["型"]) && !previous_string_is("返戻") {
        return Some(("カナ", None));
    }
    if surface == "兵" && following_string_in(&["ども", "共"]) {
        return Some(("ツワモノ", None));
    }
    if surface == "如何" && following_string_in(&["で", "です", "でし", "でしょ"]) {
        return Some(("イカガ", None));
    }

    // 直前の語が読みを確定する敬称・仏号・定型表現を表層形で判定する
    if surface == "仏" && previous_string_in(&["阿弥陀", "釈迦", "大日", "薬師", "毘盧遮那"])
    {
        return Some(("ブツ", None));
    }
    if surface == "方"
        && feature.pos_group1 == "接尾"
        && previous_string_in(HONORIFIC_PLURAL_PREDECESSORS)
    {
        return Some(("ガタ", None));
    }
    // 「就学前の児童」は句として「マエ」、「就学前教育」は複合語として「ゼン」と読む
    if surface == "前"
        && previous_is_noun_of(&["サ変接続"])
        && following.is_some_and(|f| f.pos == "助詞")
    {
        return Some(("マエ", None));
    }
    if surface == "前" && previous_string_in(ZEN_PREDECESSORS) {
        return Some(("ゼン", None));
    }
    // 「前会長」「前首相」のように直後に役職が続く「前」は、前任を表す「ゼン」と読む
    if surface == "前" && following_string_in(ZEN_SUCCESSORS) {
        return Some(("ゼン", None));
    }
    if surface == "様" && previous_string_is("同じ") {
        return Some(("ヨウ", Some("ヨー")));
    }
    if surface == "下"
        && feature.pos_group1 == "一般"
        && previous_string_is("の")
        && previous_previous_string_in(ABSTRACT_NO_PREDECESSORS)
    {
        return Some(("モト", None));
    }

    // 「橋」は構造種別なら「キョウ」（「キョー」）と読ませ、名詞に続く接尾辞用法は「バシ」と読ませる
    if surface == "橋" && previous_string_in(BRIDGE_TYPE_PREDECESSORS) {
        return Some(("キョウ", Some("キョー")));
    }
    if surface == "橋" && feature.pos_group1 == "接尾" && previous.is_some_and(|f| f.pos == "名詞")
    {
        return Some(("バシ", None));
    }
    if surface == "寺" && previous_string_in(DERA_PREDECESSORS) {
        return Some(("デラ", None));
    }
    // 「寺」の読みは前接語によって「ジ」と「デラ」に分かれるため、実証済みの複合語だけを閉じた集合で
    // 「ジ」へ補正する
    if surface == "寺" && previous_string_in(JI_PREDECESSORS) {
        return Some(("ジ", None));
    }
    // 「受者」の「受」も動詞として解析されるため、1文字の語幹に続く「者」は辞書の読みを保つ
    if surface == "者"
        && previous.is_some_and(|f| f.pos_group1 == "自立" && f.string.chars().count() > 1)
    {
        return Some(("モノ", None));
    }
    if surface == "入っ"
        && previous.is_some_and(|f| {
            f.pos == "動詞"
                && f.cform == "連用形"
                && IRU_COMPOUND_PREDECESSORS.contains(&&*f.string)
        })
    {
        return Some(("イッ", None));
    }

    // 「寺小屋」は「テラコヤ」、「いつか公になる」は「オーヤケ」と読み、名詞の後の一律の補正から分ける
    if surface == "小屋" && previous_string_is("寺") {
        return Some(("コヤ", None));
    }
    if surface == "公"
        && feature.pos_group1 == "一般"
        && previous.is_some_and(|f| f.pos_group1 == "副詞可能")
    {
        return Some(("オオヤケ", Some("オーヤケ")));
    }

    // 「大津波」と「御言葉」は同じ文脈IDの候補をコストだけでは使い分けられないため、後続語で読みを選ぶ
    if surface == "大" && feature.pos_group1 == "名詞接続" && following_string_in(OO_SUCCESSORS)
    {
        return Some(("オオ", Some("オー")));
    }
    if surface == "御" && feature.pos_group1 == "名詞接続" && following_string_in(O_SUCCESSORS)
    {
        return Some(("オ", None));
    }

    // 「柄」は同じ名詞の候補に「ガラ」「エ」「ツカ」があるので、「の」の前の道具名で分ける
    if surface == "柄" && previous_string_is("の") && previous_previous.is_some() {
        if previous_previous_string_in(TOOL_HANDLE_PREDECESSORS) {
            return Some(("エ", None));
        }
        if previous_previous_string_in(SWORD_HILT_PREDECESSORS) {
            return Some(("ツカ", None));
        }
        return None;
    }
    if surface == "縁"
        && previous_string_is("の")
        && previous_previous_string_in(FUCHI_PREDECESSORS)
    {
        return Some(("フチ", None));
    }

    // 「尼」の前が名詞という条件だけでは「毎日尼を見る」まで「ニ」になるため、実証済みの仏教語に限る
    if surface == "尼" && previous_string_in(&["修道", "比丘"]) {
        return Some(("ニ", None));
    }
    if surface == "茶屋" && previous_is_noun_of(&["一般", "固有名詞", "サ変接続"]) {
        return Some(("ジャヤ", None));
    }
    // 「芭蕉翁」は「オー」と読むが、「明日翁が来る」のような独立用法は「オキナ」を保つ
    if surface == "翁" && previous_is_noun_of(&["一般", "固有名詞", "サ変接続"]) {
        return Some(("オウ", Some("オー")));
    }

    // コストを下げても「処」が「ショ」に戻る文があるため、活用語に続く場所の「トコロ」は文脈で確定する
    if surface == "処" && previous.is_some_and(|f| f.pos_group1 == "自立" || f.pos == "助動詞")
    {
        return Some(("トコロ", None));
    }

    // 「識って」「仰しゃる」「て了った」は動詞の異表記として読む
    if surface == "於" && following_string_in(&["て", "ては", "ても"]) {
        return Some(("オイ", None));
    }
    if surface == "識" && following_string_in(&["って", "った", "り"]) {
        return Some(("シ", None));
    }
    // MeCabは「仰しゃる」を「仰」「しゃ」「る」に分けるため、直後の「しゃ」も対象に含める
    if surface == "仰"
        && following_string_in(&["しゃ", "しゃっ", "しゃる", "しゃい", "しゃら", "しゃり"])
    {
        return Some(("オッ", None));
    }
    if surface == "了" && previous_string_in(&["て", "で"]) {
        return Some(("シマ", None));
    }

    // 「より外にない」は選択肢の「ホカ」、物体の位置を比べる場合は「ソト」と読む
    if surface == "外"
        && previous_string_is("より")
        && previous_previous.is_some_and(|f| f.pos == "動詞" || f.pos_group1 == "代名詞")
    {
        for later in &features[index + 1..] {
            if ["。", "．", "！", "？"].contains(&&*later.string) {
                break;
            }
            if NEGATIVE_ORIGINALS.contains(&&*later.orig) {
                return Some(("ホカ", None));
            }
        }
        return None;
    }

    // 学位の「博士」は「ハクシ」と読み、人を指す「広瀬博士」の「ハカセ」を保つ
    if surface == "博士"
        && (following_string_in(&["学位", "論文", "号", "課程"])
            || previous_string_in(&[
                "医学",
                "工学",
                "理学",
                "農学",
                "薬学",
                "文学",
                "法学",
                "経済学",
                "大学院",
            ]))
    {
        return Some(("ハクシ", None));
    }

    // 名詞へ直接続く後部要素は、助詞を挟んだ独立用法と区別して複合語の読みへ変える
    // 数詞に続く「部屋」は助数詞なので、NJDの数詞処理が選んだ「ヘヤ」を保つ
    if let Some(&(_, reading, pronunciation, required_pos_group1)) = COMPOUND_SUFFIX_READINGS
        .iter()
        .find(|(word, ..)| *word == surface)
        && previous.is_some_and(|f| f.pos == "名詞" && (surface != "部屋" || f.pos_group1 != "数"))
    {
        return required_pos_group1
            .is_none_or(|group| feature.pos_group1 == group)
            .then_some((reading, Some(pronunciation)));
    }
    // 辞書が人名と解析する「記念章」だけを「ショウ」へ補正し、人名の「章」は「アキラ」と読むように残す
    if surface == "章" && feature.pos_group1 == "固有名詞" && previous_string_is("記念") {
        return Some(("ショウ", Some("ショー")));
    }

    // 「等」は代名詞に続けば「ラ」、自立した名詞に続けば「トウ」（「トー」）、活用語や形式名詞では
    // 既定の「ナド」を残す
    if surface == "等" && previous.is_some_and(|f| f.pos_group1 == "代名詞") {
        return Some(("ラ", None));
    }
    if surface == "等"
        && previous
            .is_some_and(|f| ["一般", "サ変接続", "固有名詞", "接尾"].contains(&&*f.pos_group1))
    {
        return Some(("トウ", Some("トー")));
    }
    None
}

/// 「何に」「も」の後の最初の述語が、確認できた打ち消しの用法かを判定する。
///
/// 「何にも答えてもらっていない」の補助動詞と「何にもすることができず」の可能表現をたどる。
/// それ以外の名詞や別の自立動詞へ進んだ場合は対象外とする。
fn is_negative_nannimo_context(features: &[NjdFeature], start: usize) -> bool {
    let mut predicate_found = false;
    let mut sahen_found = false;
    for index in start..features.len() {
        let feature = &features[index];
        // 読点・引用符や節をつなぐ助詞で区切り、「何にも似るが知らない」の後半の否定を切り離す
        if feature.pos == "記号" {
            return false;
        }
        if !predicate_found {
            // 「何にもしない」の「しない」が名詞と解析された場合も、この打ち消しの表現として扱う
            if index == start && feature.string == "しない" && feature.pos == "名詞" {
                return true;
            }
            if feature.pos_group1 == "接続助詞" {
                return false;
            }
            if !["動詞", "形容詞", "助動詞"].contains(&&*feature.pos) {
                sahen_found |= feature.pos_group1 == "サ変接続";
                continue;
            }
            // 「何にも依存しない」の「し」は「依存する」の一部なので、格助詞の「に」を保つ
            if !NANNIMO_PREDICATES.contains(&&*feature.orig)
                || (feature.orig == "する" && sahen_found)
            {
                return false;
            }
            predicate_found = true;
        } else if ["こと", "事"].contains(&&*feature.string)
            && feature.pos_group1 == "非自立"
            && index + 2 < features.len()
            && features[index + 1].string == "が"
            && ["でき", "出来"].contains(&&*features[index + 2].string)
        {
            // 「何にもすることができず」の「ことが」に続く可能の述語と、その否定まで確かめる
            return is_negative_nannimo_context(features, index + 2);
        } else if !(feature.pos == "助動詞"
            || (feature.pos == "動詞" && feature.pos_group1 == "非自立")
            || (feature.pos == "助詞" && ["て", "で", "は", "も"].contains(&&*feature.string)))
        {
            return false;
        }
        // 「ない」「無い」「知らん」「知りません」「知らず」は、原形と品詞で確認する
        if ["助動詞", "形容詞"].contains(&&*feature.pos)
            && NEGATIVE_ORIGINALS.contains(&&*feature.orig)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::test_util::parse_nodes;

    #[rstest]
    // 何にも知らない
    #[case(
        "何 名詞 代名詞 * * 何 ナニ ナニ 1 2 C3 -1
         に 助詞 格助詞 * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         も 助詞 係助詞 * * も モ モ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/副詞%F2@0/助詞%F2@0 1
         知ら 動詞 自立 五段・ラ行 未然形 知る シラ シラ 3 2 * 0
         ない 助動詞 * 特殊・ナイ 基本形 ない ナイ ナイ 1 2 助詞%F2@1/形容詞%F2@1/助動詞%F2@1/動詞%F2@1 1",
        &[("ナン", 1, 2), ("ニ", 0, 1), ("モ", 0, 1), ("シラ", 3, 2), ("ナイ", 1, 2)],
    )]
    // 何にも似ていない
    #[case(
        "何 名詞 代名詞 * * 何 ナニ ナニ 1 2 C3 -1
         に 助詞 格助詞 * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         も 助詞 係助詞 * * も モ モ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/副詞%F2@0/助詞%F2@0 1
         似 動詞 自立 一段 連用形 似る ニ ニ 4 1 * 0
         て 助詞 接続助詞 * * て テ テ 0 1 動詞%F1/形容詞%F1/名詞%F5 1
         い 動詞 非自立 一段 未然形 いる イ イ 0 1 * 1
         ない 助動詞 * 特殊・ナイ 基本形 ない ナイ ナイ 1 2 助詞%F2@1/形容詞%F2@1/助動詞%F2@1/動詞%F2@1 1",
        &[("ナニ", 1, 2), ("ニ", 0, 1), ("モ", 0, 1), ("ニ", 4, 1), ("テ", 0, 1), ("イ", 0, 1), ("ナイ", 1, 2)],
    )]
    // 何にも依存していない
    #[case(
        "何 名詞 代名詞 * * 何 ナニ ナニ 1 2 C3 -1
         に 助詞 格助詞 * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         も 助詞 係助詞 * * も モ モ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/副詞%F2@0/助詞%F2@0 1
         依存 名詞 サ変接続 * * 依存 イゾン イゾン 7 3 C2 0
         し 動詞 自立 サ変・スル 連用形 する シ シ’ 0 1 * 1
         て 助詞 接続助詞 * * て テ テ 0 1 動詞%F1/形容詞%F1/名詞%F5 1
         い 動詞 非自立 一段 未然形 いる イ イ 0 1 * 1
         ない 助動詞 * 特殊・ナイ 基本形 ない ナイ ナイ 1 2 助詞%F2@1/形容詞%F2@1/助動詞%F2@1/動詞%F2@1 1",
        &[("ナニ", 1, 2), ("ニ", 0, 1), ("モ", 0, 1), ("イゾン", 7, 3), ("シ’", 0, 1), ("テ", 0, 1), ("イ", 0, 1), ("ナイ", 1, 2)],
    )]
    // 宮沢湖
    #[case(
        "宮沢 名詞 固有名詞 * * 宮沢 ミヤザワ ミヤザワ 4 4 C2 -1
         湖 名詞 一般 * * 湖 ミズウミ ミズウミ 3 4 C3 1",
        &[("ミヤザワ", 4, 4), ("コ", 3, 1)],
    )]
    // 西湖
    #[case(
        "西湖 名詞 固有名詞 * * 西湖 サイコ サイコ 1 3 C1 -1",
        &[("サイコ", 1, 3)],
    )]
    // 先生方
    #[case(
        "先生 名詞 一般 * * 先生 センセイ センセー 4 4 C1 -1
         方 名詞 接尾 * * 方 カタ カタ 1 2 C3 1",
        &[("センセー", 4, 4), ("ガタ", 1, 2)],
    )]
    // 一見さんです
    #[case(
        "一見 名詞 一般 * * 一見 イチゲン イチゲン 7 4 C2 -1
         さん 名詞 接尾 * * さん サン サン 0 2 -1 1
         です 助動詞 * 特殊・デス 基本形 です デス デス’ 1 2 助動詞%F2@0/助詞%F2@0/連体詞%F2@0/名詞%F2@1/動詞%F1/形容詞%F2@0 1",
        &[("イチゲン", 7, 4), ("サン", 0, 2), ("デス’", 1, 2)],
    )]
    // 前会長の話です
    #[case(
        "前 名詞 副詞可能 * * 前 マエ マエ 1 2 C1 -1
         会長 名詞 一般 * * 会長 カイチョウ カイチョー 0 4 C2 0
         の 助詞 連体化 * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         話 名詞 サ変接続 * * 話 ハナシ ハナシ 3 3 C2 0
         です 助動詞 * 特殊・デス 基本形 です デス デス’ 1 2 助動詞%F2@0/助詞%F2@0/連体詞%F2@0/名詞%F2@1/動詞%F1/形容詞%F2@0 1",
        &[("ゼン", 1, 2), ("カイチョー", 0, 4), ("ノ", 1, 1), ("ハナシ", 3, 3), ("デス’", 1, 2)],
    )]
    // 就学前の児童
    #[case(
        "就学 名詞 サ変接続 * * 就学 シュウガク シューガク 5 4 C2 -1
         前 名詞 接尾 * * 前 マエ マエ 1 2 C1 1
         の 助詞 連体化 * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         児童 名詞 一般 * * 児童 ジドウ ジドー 1 3 C1 0",
        &[("シューガク", 5, 4), ("マエ", 1, 2), ("ノ", 1, 1), ("ジドー", 1, 3)],
    )]
    // そう思うより外にない
    #[case(
        "そう 副詞 * * * そう ソウ ソー 0 2 * -1
         思う 動詞 自立 五段・ワ行促音便 基本形 思う オモウ オモウ 2 3 * 0
         より 助詞 格助詞 * * より ヨリ ヨリ 1 2 名詞%F2@1 1
         外 名詞 一般 * * 外 ソト ソト 1 2 C3 0
         に 助詞 格助詞 * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         ない 形容詞 自立 形容詞・アウオ段 基本形 ない ナイ ナイ 1 2 * 0",
        &[("ソー", 0, 2), ("オモウ", 2, 3), ("ヨリ", 1, 2), ("ホカ", 1, 2), ("ニ", 0, 1), ("ナイ", 1, 2)],
    )]
    // 地球より外に惑星はない
    #[case(
        "地球 名詞 一般 * * 地球 チキュウ チ’キュー 4 3 C2 -1
         より 助詞 格助詞 * * より ヨリ ヨリ 1 2 名詞%F2@1 1
         外 名詞 一般 * * 外 ソト ソト 1 2 C3 0
         に 助詞 格助詞 * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         惑星 名詞 一般 * * 惑星 ワクセイ ワク’セー 0 4 C2 0
         は 助詞 係助詞 * * は ハ ワ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/助詞%F2@0 1
         ない 形容詞 自立 形容詞・アウオ段 基本形 ない ナイ ナイ 1 2 * 0",
        &[("チ’キュー", 4, 3), ("ヨリ", 1, 2), ("ソト", 1, 2), ("ニ", 0, 1), ("ワク’セー", 0, 4), ("ワ", 0, 1), ("ナイ", 1, 2)],
    )]
    // 傘の柄を握る
    #[case(
        "傘 名詞 一般 * * 傘 カサ カサ 1 2 C3 -1
         の 助詞 連体化 * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         柄 名詞 一般 * * 柄 ガラ ガラ 0 2 C4 0
         を 助詞 格助詞 * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         握る 動詞 自立 五段・ラ行 基本形 握る ニギル ニギル 0 3 * 0",
        &[("カサ", 1, 2), ("ノ", 1, 1), ("エ", 0, 1), ("ヲ", 0, 1), ("ニギル", 0, 3)],
    )]
    // 低解約返戻金型終身保険
    #[case(
        "低 接頭詞 名詞接続 * * 低 テイ テー 1 2 P2 -1
         解約 名詞 サ変接続 * * 解約 カイヤク カイヤク’ 0 4 C2 1
         返戻 名詞 サ変接続 * * 返戻 ヘンレイ ヘンレー 0 4 C2 1
         金 名詞 接尾 * * 金 キン キン 1 2 C4 1
         型 名詞 接尾 * * 型 ガタ ガタ 2 2 C4 1
         終身 名詞 一般 * * 終身 シュウシン シューシン 5 4 C2 0
         保険 名詞 一般 * * 保険 ホケン ホケン 0 3 C2 1",
        &[("テー", 1, 2), ("カイヤク’", 0, 4), ("ヘンレー", 0, 4), ("キン", 1, 2), ("ガタ", 2, 2), ("シューシン", 5, 4), ("ホケン", 0, 3)],
    )]
    // 資金不足
    #[case(
        "資金不足 名詞 一般 * * 資金不足 シキンブソク シ’キンブソク 4 6 C1 -1",
        &[("シ’キンブソク", 4, 6)],
    )]
    // 十八角形
    #[case(
        "十 名詞 数 * * 十 ジュウ ジュー 5 2 C3 -1
         八 名詞 数 * * 八 ハチ ハチ’ 2 2 C1 1
         角形 名詞 一般 * * 角形 カクガタ カクガタ 0 4 C2 1",
        &[("ジュー", 5, 2), ("ハチ’", 2, 2), ("カクケー", 0, 4)],
    )]
    // 貴乃花部屋
    #[case(
        "貴乃花 名詞 固有名詞 * * 貴乃花 タカノハナ タカノハナ 5 5 C1 -1
         部屋 名詞 一般 * * 部屋 ヘヤ ヘヤ 2 2 C3 1",
        &[("タカノハナ", 5, 5), ("ベヤ", 2, 2)],
    )]
    // 機器等
    #[case(
        "機器 名詞 一般 * * 機器 キキ キ’キ 2 2 C1 -1
         等 名詞 一般 * * 等 ナド ナド 1 2 C3 1",
        &[("キ’キ", 2, 2), ("トー", 1, 2)],
    )]
    // それ等
    #[case(
        "それ 名詞 代名詞 * * それ ソレ ソレ 0 2 C3 -1
         等 名詞 一般 * * 等 ナド ナド 1 2 C3 0",
        &[("ソレ", 0, 2), ("ラ", 1, 1)],
    )]
    // 受者の責任
    #[case(
        "受 動詞 自立 一段 連用形 受る ウケ ウケ 2 2 C1 -1
         者 名詞 接尾 * * 者 シャ シャ 1 1 C3 1
         の 助詞 連体化 * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         責任 名詞 一般 * * 責任 セキニン セキニン 0 4 C2 0",
        &[("ウケ", 2, 2), ("シャ", 1, 1), ("ノ", 1, 1), ("セキニン", 0, 4)],
    )]
    // 余呉湖
    #[case(
        "余呉湖 名詞 固有名詞 * * 余呉湖 ヨゴコ ヨゴコ 2 3 C1 -1",
        &[("ヨゴコ", 2, 3)],
    )]
    // 塩湖
    #[case(
        "塩 名詞 一般 * * 塩 シオ シオ 5 2 C3 -1
         湖 名詞 一般 * * 湖 ミズウミ ミズウミ 3 4 C1 1",
        &[("シオ", 2, 2), ("コ", 3, 1)],
    )]
    // 識っている
    #[case(
        "識 名詞 一般 * * 識 シキ シ’キ 2 2 C3 -1
         って 助詞 終助詞 * * って ッテ ッテ 0 2 名詞%F1/動詞%F2@0/形容詞%F1 1
         いる 動詞 非自立 一段 基本形 いる イル イル 0 2 助詞%F2@2/動詞%F2@1 1",
        &[("シ", 1, 1), ("ッテ", 0, 2), ("イル", 0, 2)],
    )]
    // 一声かける
    #[case(
        "一声 名詞 一般 * * 一声 イッセイ イッセー 0 4 C2 -1
         かける 動詞 自立 一段 基本形 かける カケル カケル 2 3 * 0",
        &[("ヒトコエ", 0, 4), ("カケル", 2, 3)],
    )]
    // 何にもすることができず
    #[case(
        "何 名詞 代名詞 * * 何 ナニ ナニ 1 2 C3 -1
         に 助詞 格助詞 * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         も 助詞 係助詞 * * も モ モ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/副詞%F2@0/助詞%F2@0 1
         する 動詞 自立 サ変・スル 基本形 する スル スル 4 2 * 0
         こと 名詞 非自立 * * こと コト コト 1 2 名詞%F2@1/動詞%F2@2/助詞%F2@1/連体詞%F2@1/助動詞%F2@1 1
         が 助詞 格助詞 * * が ガ ガ 0 1 名詞%F1 1
         でき 動詞 自立 一段 未然形 する デキ デキ 2 2 * 0
         ず 助動詞 * 特殊・ヌ 連用ニ接続 ぬ ズ ズ 1 1 動詞%F2@0 1",
        &[("ナン", 1, 2), ("ニ", 0, 1), ("モ", 0, 1), ("スル", 4, 2), ("コト", 1, 2), ("ガ", 0, 1), ("デキ", 2, 2), ("ズ", 1, 1)],
    )]
    fn modify_context_reading_works(#[case] features: &str, #[case] expected: &[(&str, i32, i32)]) {
        let actual = super::modify_context_reading(parse_nodes(features));
        assert_eq!(
            expected,
            actual
                .iter()
                .map(|f| (&*f.pron, f.acc, f.mora_size))
                .collect::<Vec<_>>(),
        );
    }

    /// 「識っている」の「シキ」を「シ」へ縮めても、後続するモーラにあったアクセント核の位置を保つ。
    #[rstest]
    #[case(0, 0)]
    #[case(1, 1)]
    #[case(2, 1)]
    #[case(4, 3)]
    fn modify_context_reading_keeps_nucleus_when_mora_count_changes(
        #[case] accent: i32,
        #[case] expected: i32,
    ) {
        let mut features = parse_nodes(
            "識 名詞 一般 * * 識 シキ シ’キ 2 2 C3 -1
             って 助詞 終助詞 * * って ッテ ッテ 0 2 * 1
             いる 動詞 非自立 一段 基本形 いる イル イル 0 2 * 1",
        );
        features[0].acc = accent;
        let actual = super::modify_context_reading(features);
        assert_eq!(
            ("シ", 1, expected),
            (&*actual[0].pron, actual[0].mora_size, actual[0].acc)
        );
    }

    /// 既に「ハクシ」と読める「博士」は、無声化の記号もそのまま残す。
    #[test]
    fn modify_context_reading_preserves_devoicing() {
        let features = parse_nodes(
            "博士 名詞 一般 * * 博士 ハクシ ハク’シ 1 3 C1 -1
             論文 名詞 サ変接続 * * 論文 ロンブン ロンブン 0 4 C1 1",
        );
        let actual = super::modify_context_reading(features);
        assert_eq!("ハク’シ", actual[0].pron);
    }
}
