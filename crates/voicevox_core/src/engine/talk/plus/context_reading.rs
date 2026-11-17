// Ported from pyopenjtalk-plus (MIT) https://github.com/tsukumijima/pyopenjtalk-plus @ v0.4.1-post10 (b312c6f)
// (`pyopenjtalk/_context_reading.py`)

//! 前後の形態素で読みが決まる語（「駆け込み寺」の「デラ」、「先生方」の「ガタ」など）の読みを
//! 書き換える。
//!
//! 辞書の生起コストだけでは使い分けられない語を、前後の語・品詞・同じ文の語から判定して読みを
//! 書き換える。規則は1つの形態素ごとに順に試し、同じ表層形に読みの候補が複数ある規則は、最初に
//! 当たった規則だけを適用する。
//!
//! pyopenjtalk-plusの読み保護（`is_reading_protected`）は扱わず、常に保護されていないものとする。

use open_jtalk::NjdFeature;

use super::kana::mora_count;

// 文脈による読み補正で、前後の語から読みを決めるときに使う語の集合

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
///
/// 「視入る」「駆け入る」「乱れ入る」も複合動詞として「イル」と読む（動詞の連用形一般には広げず、
/// 「怒り入ってる」のような名詞用法を巻き込まない）。
const IRU_COMPOUND_PREDECESSORS: &[&str] =
    &["走り", "攻め", "折り", "分け", "視", "駆け", "駈け", "乱れ"];

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

/// 「盲」「聾」の音読みを、学校種別や障害種別を表す語との列挙に限って適用する。
const DISABILITY_ENUMERATION_TERMS: &[&str] = &[
    "盲",
    "聾",
    "ろう",
    "盲学校",
    "聾学校",
    "ろう学校",
    "養護学校",
    "視覚障害",
    "聴覚障害",
    "知的障害",
    "肢体不自由",
    "病弱",
];

/// 空間を比較する「より外」は「ソト」なので、「ホカ」への補正は動詞・代名詞と打ち消しの組に限る。
const NEGATIVE_ORIGINALS: &[&str] = &["ない", "無い", "ぬ", "ん", "まい", "ず"];

/// 「何にも知らない」「何にもならない」のように、打ち消しと組んで「ナンニモ」と読む述語に限る。
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

/// 直前か直後の語の表層形だけで読みが決まる語の読み（表層形、品詞細分類の条件、直前の語、
/// 直後の語、読み、発音）。
///
/// 品詞細分類・直前の語・直後の語の`None`はその条件を問わず、発音の`None`は読みと同じ発音に
/// することを表す。同じ表層形の行は上から順に試し、最初に当たった行の読みを使う。
type AdjacentWordReading = (
    &'static str,
    Option<&'static str>,
    Option<&'static [&'static str]>,
    Option<&'static [&'static str]>,
    &'static str,
    Option<&'static str>,
);

const ADJACENT_WORD_READINGS: &[AdjacentWordReading] = &[
    // 直後の語で意味が確定する少数の表現を、閉じた表層形の集合で判定する
    ("一見", None, None, Some(&["さん"]), "イチゲン", None),
    (
        "一声",
        None,
        None,
        Some(&["かけ", "掛け", "かける", "掛ける"]),
        "ヒトコエ",
        None,
    ),
    (
        "一行",
        None,
        None,
        Some(&["ごと", "毎"]),
        "イチギョウ",
        Some("イチギョー"),
    ),
    ("町", None, None, Some(&["史"]), "チョウ", Some("チョー")),
    ("一", None, None, Some(&["しょ"]), "イッ", None),
    ("兵", None, None, Some(&["ども", "共"]), "ツワモノ", None),
    (
        "如何",
        None,
        None,
        Some(&["で", "です", "でし", "でしょ"]),
        "イカガ",
        None,
    ),
    // 「保護房」の「房」は「ブドウの房」と同じ品詞の候補なので、前接語で区別する
    ("房", None, Some(&["保護"]), None, "ボウ", Some("ボー")),
    // 「五月期」の「五月」は月名なので、植物の「サツキ」の読みを保つため後続語を限定する
    ("五月", None, None, Some(&["期"]), "ゴガツ", None),
    // 大会名・学校名・行政機関名の前に付く「都」は「ト」と読み、都市を表す「ミヤコ」を保つ
    (
        "都",
        None,
        None,
        Some(&["大会", "戸山", "公安", "副知事"]),
        "ト",
        None,
    ),
    // 「露対テロ」の「露」はロシアの略称なので、「ツユ」と区別して「ロ」と読む
    ("露", None, None, Some(&["対"]), "ロ", None),
    // 「十勝した」は勝利数を表し、地名の「十勝」の読みは維持する
    (
        "十勝",
        None,
        None,
        Some(&["し", "したん", "する"]),
        "ジュッショウ",
        Some("ジュッショー"),
    ),
    // 直前の語が読みを確定する敬称・仏号・定型表現を表層形で判定する
    (
        "仏",
        None,
        Some(&["阿弥陀", "釈迦", "大日", "薬師", "毘盧遮那"]),
        None,
        "ブツ",
        None,
    ),
    // tsqyomiは「あなた方」の「方」を非自立の「ホウ」の行に差し替えるので、品詞細分類は問わず、
    // 敬称・複数の前接語だけで判定する
    (
        "方",
        None,
        Some(HONORIFIC_PLURAL_PREDECESSORS),
        None,
        "ガタ",
        None,
    ),
    ("前", None, Some(ZEN_PREDECESSORS), None, "ゼン", None),
    // 「前会長」「前首相」のように直後に役職が続く「前」は、前任を表す「ゼン」と読む
    ("前", None, None, Some(ZEN_SUCCESSORS), "ゼン", None),
    ("様", None, Some(&["同じ"]), None, "ヨウ", Some("ヨー")),
    // 「橋」は構造種別なら「キョウ」（「キョー」）と読ませる（名詞に続く接尾辞用法の「バシ」は
    // `read_bashi_suffix`で読ませる）
    (
        "橋",
        None,
        Some(BRIDGE_TYPE_PREDECESSORS),
        None,
        "キョウ",
        Some("キョー"),
    ),
    ("寺", None, Some(DERA_PREDECESSORS), None, "デラ", None),
    // 「寺」の読みは前接語によって「ジ」と「デラ」に分かれるため、実証済みの複合語だけを閉じた
    // 集合で「ジ」へ補正する
    ("寺", None, Some(JI_PREDECESSORS), None, "ジ", None),
    // 「寺小屋」は「テラコヤ」と読み、名詞の後の一律の補正から分ける
    ("小屋", None, Some(&["寺"]), None, "コヤ", None),
    // 「大津波」と「御言葉」は同じ文脈IDの候補をコストだけでは使い分けられないため、後続語で読みを
    // 選ぶ
    (
        "大",
        Some("名詞接続"),
        None,
        Some(OO_SUCCESSORS),
        "オオ",
        Some("オー"),
    ),
    ("御", Some("名詞接続"), None, Some(O_SUCCESSORS), "オ", None),
    // 「尼」の前が名詞という条件だけでは「毎日尼を見る」まで「ニ」になるため、実証済みの仏教語に
    // 限る
    ("尼", None, Some(&["修道", "比丘"]), None, "ニ", None),
    // 「識って」「て了った」は動詞の異表記として読む
    // 「仰しゃる」「仰っしゃる」は辞書に全活用形の行があるので、規則では読み替えない
    (
        "於",
        None,
        None,
        Some(&["て", "ては", "ても"]),
        "オイ",
        None,
    ),
    ("識", None, None, Some(&["って", "った", "り"]), "シ", None),
    ("了", None, Some(&["て", "で"]), None, "シマ", None),
    // 学位の「博士」は「ハクシ」と読み、人を指す「広瀬博士」の「ハカセ」を保つ
    (
        "博士",
        None,
        None,
        Some(&["学位", "論文", "号", "課程"]),
        "ハクシ",
        None,
    ),
    (
        "博士",
        None,
        Some(&[
            "医学",
            "工学",
            "理学",
            "農学",
            "薬学",
            "文学",
            "法学",
            "経済学",
            "大学院",
        ]),
        None,
        "ハクシ",
        None,
    ),
    // 辞書が人名と解析する「記念章」だけを「ショウ」へ補正し、人名の「章」は「アキラ」と読むように
    // 残す
    (
        "章",
        Some("固有名詞"),
        Some(&["記念"]),
        None,
        "ショウ",
        Some("ショー"),
    ),
];

/// 前後の語との組み合わせで読みが決まる規則。当たったものをすべて同じ形態素に重ねて適用する。
const INDEPENDENT_RULES: &[fn(&mut [NjdFeature], usize)] = &[
    read_hair_tying_motoyui,
    read_verdict_kokubyaku,
    read_awakening_kaigen,
    read_sonohoka,
    read_comparative_yoi,
    read_figurative_asu,
    read_petal_kaben,
    read_dull_niburu,
    read_water_surface_omote,
    read_ametsuchi,
    read_municipal_enumeration,
    read_origin_moto,
    read_water_master_nushi,
    read_musashi_name,
    read_old_loom_hata,
    read_central_kaname,
    read_melodic_fushi,
    read_enduring_taeru,
    read_shrine_yashiro,
    read_impurity_kegare,
    read_giving_up_ne,
    read_cloth_beniiro,
    read_disability_enumeration,
    read_ichiban_suki,
];

/// 同じ表層形に読みの候補が複数ある規則。上から順に試し、最初に当たった規則だけを適用する。
///
/// 各規則は、条件に当たって後続の規則を試さない場合に`true`を返す。
const EXCLUSIVE_RULES: &[fn(&mut [NjdFeature], usize) -> bool] = &[
    read_nannimo,
    read_face_omote,
    read_capsized_kutsugae,
    read_edge_kiwa,
    read_chanted_shomyo,
    read_hemp_cloth_asanuno,
    read_age_before_twenty,
    read_age_past_twenty,
    read_counted_too,
    read_all_day_juu,
    read_kin,
    read_mae_after_sahen_noun,
    read_by_adjacent_word,
    read_futsu_parenthetical_country,
    read_kamei_after_person_name,
    read_to_administration,
    read_you_auxiliary,
    read_kami_era_yo,
    read_abstract_moto,
    read_digit_position,
    read_bashi_suffix,
    read_mono,
    read_compound_iru,
    read_leave_houtte,
    read_ooyake,
    read_handle_e_or_tsuka,
    read_rim_fuchi,
    read_teahouse_jaya,
    read_honorific_ou,
    read_tokoro,
    read_shiita,
    read_sourou,
    read_hoka,
    read_compound_suffix,
    read_ra_or_tou,
];

/// 前後の形態素で読みが決まる語（「駆け込み寺」の「デラ」、「先生方」の「ガタ」など）の読みを
/// 書き換える。
///
/// 「より外にない」の「ホカ」は、後続する打ち消しの語も確かめてから適用する。
pub(super) fn modify_context_reading(mut features: Vec<NjdFeature>) -> Vec<NjdFeature> {
    for index in 0..features.len() {
        for rule in INDEPENDENT_RULES {
            rule(&mut features, index);
        }
        for rule in EXCLUSIVE_RULES {
            if rule(&mut features, index) {
                break;
            }
        }
    }
    features
}

/// `index`から`offset`だけ離れた形態素。形態素列の範囲外なら`None`。
fn feature_at(features: &[NjdFeature], index: usize, offset: isize) -> Option<&NjdFeature> {
    index
        .checked_add_signed(offset)
        .and_then(|position| features.get(position))
}

/// `index`から`offset`だけ離れた形態素の表層形が`words`に含まれるか。
fn string_at_in(features: &[NjdFeature], index: usize, offset: isize, words: &[&str]) -> bool {
    feature_at(features, index, offset).is_some_and(|f| words.contains(&&*f.string))
}

/// 先頭の`n`文字を除いた残り。
fn skip_chars(text: &str, n: usize) -> &str {
    text.char_indices().nth(n).map_or("", |(i, _)| &text[i..])
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

/// アクセント句の先頭にある語のアクセント核を設定し、同じ句に結合した後続の助詞・助動詞の
/// 結合規則を計算し直す。
///
/// 「など」「より」「です」のように平板型の語に続くと核を自分の上へ移す助詞があるため、語の核
/// だけを書き換えると、句の核の位置が平板型かどうかの変化に追従しない。
///
/// `accent`は語の中でのアクセント核の位置（0は平板型）。
fn set_accent(features: &mut [NjdFeature], index: usize, accent: i32) {
    features[index].acc = accent;
    // NJDの結合規則と同じく、F2は平板型のとき、F3は起伏型のとき、F4は常に、核を句のそれまでの
    // モーラ数に加算位置を足した位置へ移し、F5とC4は平板型にする
    // NJDは直前の語の品詞名に規則の品詞名が含まれるかで照合するので、「助動詞」の後では「動詞」の
    // 規則も当たる
    let mut phrase_mora_size = features[index].mora_size;
    for cursor in index + 1..features.len() {
        let node = &features[cursor];
        if node.chain_flag != 1 || !["助詞", "助動詞"].contains(&&*node.pos) {
            break;
        }
        let previous_pos = &features[cursor - 1].pos;
        let mut new_accent = None;
        for rule in node.chain_rule.split('/') {
            let suffix = match rule.split_once('%') {
                Some((part_of_speech, suffix)) => {
                    if !previous_pos.contains(part_of_speech) {
                        continue;
                    }
                    suffix
                }
                None => rule,
            };
            let (rule_name, offset) = suffix.split_once('@').unwrap_or((suffix, ""));
            let accent = features[index].acc;
            if (rule_name == "F2" && accent == 0)
                || (rule_name == "F3" && accent != 0)
                || rule_name == "F4"
            {
                new_accent = Some(phrase_mora_size + offset.parse::<i32>().unwrap_or(0));
            } else if ["F5", "C4"].contains(&rule_name) {
                new_accent = Some(0);
            }
            break;
        }
        phrase_mora_size += node.mora_size;
        if let Some(new_accent) = new_accent {
            features[index].acc = new_accent;
        }
    }
}

/// `index`の形態素を含む文（句点・感嘆符・疑問符で区切った範囲）。
fn sentence(features: &[NjdFeature], index: usize) -> &[NjdFeature] {
    let is_boundary = |f: &NjdFeature| ["。", "！", "？"].contains(&&*f.string);
    let start = features[..index]
        .iter()
        .rposition(is_boundary)
        .map_or(0, |i| i + 1);
    let end = features[index + 1..]
        .iter()
        .position(is_boundary)
        .map_or(features.len(), |i| index + 1 + i);
    &features[start..end]
}

/// `index`の形態素を含む文に、原形が`words`に含まれる形態素があるか。
fn sentence_has_any(features: &[NjdFeature], index: usize, words: &[&str]) -> bool {
    sentence(features, index)
        .iter()
        .any(|f| words.contains(&&*f.orig))
}

/// 髪を結う用法の「元結」だけを「モトユイ」にし、別の読みを明示した文章や複合語の一部では元の
/// 読みを保つ。
fn read_hair_tying_motoyui(features: &mut [NjdFeature], index: usize) {
    let feature = &features[index];
    // 名詞のコストを一律に下げると「もっとい」の注記がある箇所も変わるため、独立した名詞で、同じ
    // 文に髪を結う手掛かりがある場合に絞る
    if feature.string == "元結"
        && feature.pos == "名詞"
        && feature.pos_group1 == "一般"
        && feature_at(features, index, -1).is_none_or(|f| !["名詞", "接頭詞"].contains(&&*f.pos))
        && is_hair_tying_motoyui(features, index)
    {
        set_reading(features, index, "モトユイ", None);
        set_accent(features, index, 3);
    }
}

/// 読みの明示がない段落で、対象の「元結」と同じ文に髪を結う手掛かりがあるかを判定する。
fn is_hair_tying_motoyui(features: &[NjdFeature], position: usize) -> bool {
    // 読みの注記は後続する同じ語にも適用されるので、段落に「もっとい」の表記があれば元の読みを
    // 保つ
    let paragraph = features.iter().map(|f| &*f.string).collect::<String>();
    if paragraph.contains("もっとい") || paragraph.contains("モットイ") {
        return false;
    }

    // 読点では同じ文が続くため、句点・疑問符・感嘆符で範囲を区切って髪を結う手掛かりを調べる
    let is_boundary =
        |f: &NjdFeature| ["。", "？", "！", "?", "!", "\n", "\r", "\r\n"].contains(&&*f.string);
    let start = features[..position]
        .iter()
        .rposition(is_boundary)
        .map_or(0, |i| i + 1);
    let end = features[position + 1..]
        .iter()
        .position(is_boundary)
        .map_or(features.len(), |i| position + 1 + i);
    features[start..end].iter().any(|f| {
        [
            "髷",
            "髻",
            "髪",
            "黒髪",
            "頭髪",
            "長髪",
            "結髪",
            "髪結い",
            "髪結",
            "結う",
            "侍",
            "力士",
        ]
        .contains(&&*f.orig)
    })
}

/// 色の「黒白」は「クロシロ」を保ち、是非を判定する述語が続く場合だけ「コクビャク」を選ぶ。
fn read_verdict_kokubyaku(features: &mut [NjdFeature], index: usize) {
    // 写真や弔事の水引の色を表す「黒白」もあるため、「コクビャク」の生起コストを一律に下げず、
    // 格助詞と直後の述語で限定する
    if features[index].string == "黒白"
        && string_at_in(features, index, 1, &["を", "が"])
        && index + 2 < features.len()
        && ((features[index + 1].string == "を"
            && ["つける", "付ける", "争う", "決める"].contains(&&*features[index + 2].orig))
            || features[index + 2].string == "明らか")
    {
        set_reading(features, index, "コクビャク", None);
        set_accent(features, index, 0);
    }
}

/// 目を開く医療の用法は「カイガン」を保ち、技芸の習得や悟りの対象を伴う用法を「カイゲン」に
/// する。
fn read_awakening_kaigen(features: &mut [NjdFeature], index: usize) {
    // 宗教語が文中にあるだけでは眼を開く動作と区別できないため、直前の習得対象か悟りを得る述語で
    // 判定する
    // 技芸の用法にも「カイガン」の読みがあるため、直後に括弧による注記がある場合は元の読みを残す
    if features[index].string == "開眼"
        && !string_at_in(features, index, 1, &["（", "("])
        && ((string_at_in(features, index, -1, &["に"])
            && string_at_in(
                features,
                index,
                -2,
                &[
                    "真髄",
                    "技芸",
                    "悟り",
                    "境地",
                    "禅",
                    "音楽",
                    "芸術",
                    "剣術",
                    "書道",
                    "茶道",
                    "ワイン",
                    "ゴルフ",
                ],
            ))
            || (string_at_in(features, index, -1, &["て"])
                && index >= 4
                && features[index - 2].orig == "得る"
                && features[index - 3].string == "を"
                && features[index - 4].string == "悟り"))
    {
        set_reading(features, index, "カイゲン", None);
        set_accent(features, index, 0);
    }
}

/// 追加の内容を示す「その他に」は「ソノホカ」と読み、分類の名称として扱う「その他」は「ソノタ」
/// を保つ。
fn read_sonohoka(features: &mut [NjdFeature], index: usize) {
    // 「その他に分類する」も同じ助詞になるため、直後の分類・区分の動詞を確認してから補正する
    if features[index].string == "その他"
        && string_at_in(features, index, 1, &["に"])
        && (index + 2 == features.len()
            || !["分類", "区分", "分類する", "区分する", "含める", "入れる"]
                .contains(&&*features[index + 2].orig))
    {
        set_reading(features, index, "ソノホカ", None);
    }
}

/// 比較の副詞「より」に続く「良い」を「ヨイ」にし、独立した「良い」の辞書の読みを保つ。
fn read_comparative_yoi(features: &mut [NjdFeature], index: usize) {
    let surface = &*features[index].string;
    // 複合語が1つの形態素になった場合も、単独の「良い」のコスト調整では届かない同じ比較表現として
    // 扱う
    if surface == "より良い" {
        set_reading(features, index, "ヨリヨイ", None);
    } else if surface == "良い"
        && feature_at(features, index, -1).is_some_and(|f| f.string == "より" && f.pos == "副詞")
    {
        set_reading(features, index, "ヨイ", None);
    }
}

/// 将来を指す「明日の社会・国」は「アス」を選び、日付を指す「明日の会議」「明日の天気」は
/// 「アシタ」を保つ。
fn read_figurative_asu(features: &mut [NjdFeature], index: usize) {
    // 名詞全体のコストでは日付用法も変わるため、連体助詞と社会・国家を表す直後の名詞で限定する
    if features[index].string == "明日"
        && string_at_in(features, index, 1, &["の"])
        && string_at_in(features, index, 2, &["日本", "社会", "未来", "国", "世界"])
        && feature_at(features, index, 3)
            .is_none_or(|f| ["助詞", "助動詞", "記号"].contains(&&*f.pos))
    {
        set_reading(features, index, "アス", None);
        set_accent(features, index, 2);
    }
}

/// 植物の部位として独立した「花弁」を「カベン」と読み、料理名や読みの注記が続く形の「ハナビラ」を
/// 保つ。
fn read_petal_kaben(features: &mut [NjdFeature], index: usize) {
    // コストを一律に下げると料理の「花弁大根」や「花弁（はなびら）」まで変わるため、
    // 助詞・助動詞に続く形に絞る
    if features[index].string == "花弁"
        && feature_at(features, index, 1).is_some_and(|f| ["助詞", "助動詞"].contains(&&*f.pos))
    {
        let has_particle_accent = features[index].acc > features[index].mora_size;
        set_reading(features, index, "カベン", None);
        // 語の内部の核だけを平板にし、既に後続助詞にある核はモーラ数の補正後の位置を保つ
        // 平板化で「など」のF2規則が新たに働くため、結合済みの助詞・助動詞を順に計算し直す
        if !has_particle_accent {
            set_accent(features, index, 0);
        }
    }
}

/// 刃物の切れ味を述べる「鈍る」は「ニブル」と読み、身体がなまる用法の読みを保つ。
fn read_dull_niburu(features: &mut [NjdFeature], index: usize) {
    let feature = &features[index];
    // 動詞全体のコストは習得した技能や身体の用法にも届くため、切れ味を主語か話題にした形で限定する
    if feature.orig == "鈍る"
        && string_at_in(features, index, -1, &["が", "は", "も"])
        && string_at_in(features, index, -2, &["切れ味"])
        && feature.read.starts_with("ナマ")
    {
        let reading = format!("ニブ{}", skip_chars(&feature.read, 2));
        set_reading(features, index, &reading, None);
    }
}

/// 水面を指す「水の面」を「ミズノオモテ」と読み、観点を表す「水の面で」や描写の動詞を伴わない
/// 「ミノモ」を保つ。
fn read_water_surface_omote(features: &mut [NjdFeature], index: usize) {
    let feature = &features[index];
    let is_phrase = feature.string == "水の面";
    // 全体を1語にした辞書の行も扱うが、用水などの語の途中を除き、同じ節の最初の動詞が水面の描写の
    // 場合だけ補正する
    if !(((is_phrase
        && feature_at(features, index, -1).is_none_or(|f| !["名詞", "接頭詞"].contains(&&*f.pos)))
        || (feature.string == "面"
            && string_at_in(features, index, -1, &["の"])
            && string_at_in(features, index, -2, &["水"])
            && feature.pos_group1 == "一般"))
        && string_at_in(features, index, 1, &["に", "を", "が", "は"]))
    {
        return;
    }
    for node in &features[index + 2..] {
        if node.pos == "記号" {
            break;
        }
        if node.pos == "動詞" && node.pos_group1 == "自立" {
            if [
                "映る",
                "浮かぶ",
                "揺れる",
                "覆う",
                "蔽う",
                "見つめる",
                "眺める",
            ]
            .contains(&&*node.orig)
            {
                set_reading(
                    features,
                    index,
                    if is_phrase {
                        "ミズノオモテ"
                    } else {
                        "オモテ"
                    },
                    None,
                );
                // 「水」は平板で「面」は尾高なので、連語なら6モーラ目、単独の面なら3モーラ目に核を
                // 置く
                set_accent(features, index, if is_phrase { 6 } else { 3 });
            }
            break;
        }
    }
}

/// 古語の連語「天地の道」は「アメツチ」と読み、荷物の上下などを表す一般語の「天地」は「テンチ」を
/// 保つ。
fn read_ametsuchi(features: &mut [NjdFeature], index: usize) {
    // 短い連語を辞書へ足すと道具などの途中にも届くため、独立した道が続く形態素の境界で限定する
    if features[index].string == "天地"
        && string_at_in(features, index, 1, &["の"])
        && feature_at(features, index, 2)
            .is_some_and(|f| f.string == "道" && f.pos_group1 == "一般")
        && feature_at(features, index, 3)
            .is_none_or(|f| ["助詞", "助動詞", "記号"].contains(&&*f.pos))
    {
        set_reading(features, index, "アメツチ", None);
        set_accent(features, index, 1);
    }
}

/// 行政区分を市と並べる町・村は「チョウ」「ソン」と読み、単独の町・村や地名の接尾辞は辞書の読みを
/// 保つ。
fn read_municipal_enumeration(features: &mut [NjdFeature], index: usize) {
    let feature = &features[index];
    let is_town = feature.string == "町";
    // 読点や中黒で続く名詞だけを調べ、並立助詞でつないだ表現や文中の離れた市、都市名から単独の
    // 町・村を音読みへ変えることを防ぐ
    if ["町", "村"].contains(&&*feature.string)
        && feature.pos_group1 == "一般"
        && is_municipal_enumeration(features, index)
    {
        set_reading(
            features,
            index,
            if is_town { "チョウ" } else { "ソン" },
            Some(if is_town { "チョー" } else { "ソン" }),
        );
        set_accent(features, index, 1);
    }
}

/// 独立した市・町・村が読点や中黒で続く場合に、市を含む行政区分の列挙かを判定する。
fn is_municipal_enumeration(features: &[NjdFeature], position: usize) -> bool {
    let mut names = vec![&*features[position].string];
    // 「市や町」「市と村」などの普段の言い方は訓読みを保ち、読点や中黒で区分を並べた列挙だけを
    // たどる
    for step in [-1, 1] {
        let mut cursor = position as isize + step;
        while 0 <= cursor + step && ((cursor + step) as usize) < features.len() {
            let separator = &features[cursor as usize];
            let node = &features[(cursor + step) as usize];
            if !["、", "・"].contains(&&*separator.string)
                || !["市", "町", "村"].contains(&&*node.string)
                || node.pos != "名詞"
                || node.pos_group1 != "一般"
            {
                break;
            }
            if !names.contains(&&*node.string) {
                names.push(&node.string);
            }
            cursor += 2 * step;
        }
    }
    names.contains(&"市") && names.len() > 1
}

/// 成り立ちを調べる「本を正す」の「本」を「モト」と読む。
fn read_origin_moto(features: &mut [NjdFeature], index: usize) {
    // 「モト」のコストを下げると書物の「本を読む」も変わるため、成り立ちを調べる「本を正す」に絞る
    if features[index].string == "本"
        && features[index].pos == "名詞"
        && string_at_in(features, index, 1, &["を"])
        && feature_at(features, index, 2).is_some_and(|f| f.orig == "正す")
    {
        set_reading(features, index, "モト", None);
        if features[index].chain_flag != 1 {
            set_accent(features, index, 2);
        }
    }
}

/// 副詞の「主として」を一律に変えず、湖・沼・池に住む主を表す連体修飾だけで「ヌシ」と読む。
fn read_water_master_nushi(features: &mut [NjdFeature], index: usize) {
    // 名詞の「主」の読みは保持し、湖名は1語になる場合もあるため末尾の「湖」を認める
    // 「電池」なども含む末尾の「池」へは条件を広げない
    if features[index].string == "主として"
        && features[index].pos == "副詞"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.string == "の" && f.pos_group1 == "連体化")
        && feature_at(features, index, -2).is_some_and(|f| {
            f.pos == "名詞"
                && (["湖", "沼", "池"].contains(&&*f.string) || f.string.ends_with('湖'))
        })
    {
        set_reading(features, index, "ヌシトシテ", None);
        if features[index].chain_flag != 1 {
            set_accent(features, index, 1);
        }
    }
}

/// 名前の提示か人名の接尾辞がある場合だけ、数詞3語の「六三四」を1句の「ムサシ」と読む。
fn read_musashi_name(features: &mut [NjdFeature], index: usize) {
    // 「六三四」を辞書で優先すると数字列も変わるため、名前の提示か人名の接尾辞がある場合だけ
    // 補正する
    // 数詞3語を1句にまとめ、前後に数詞がある長い数字列と保護された読みはそのままにする
    let Some(digits) = features.get(index..index + 3) else {
        return;
    };
    if !(digits.iter().map(|f| &*f.string).eq(["六", "三", "四"])
        && digits.iter().all(|f| f.pos_group1 == "数")
        && feature_at(features, index, -1).is_none_or(|f| f.pos_group1 != "数")
        && feature_at(features, index, 3).is_none_or(|f| f.pos_group1 != "数")
        && ((string_at_in(features, index, -1, &["は", "が"])
            && string_at_in(features, index, -2, &["名前", "氏名", "名"]))
            || feature_at(features, index, 3).is_some_and(|f| {
                ["くん", "君", "さん"].contains(&&*f.string) && f.pos_group1 == "接尾"
            })
            || (index + 5 < features.len()
                && features[index + 3].string == "と"
                && features[index + 4].orig == "いう"
                && ["名前", "氏名", "名"].contains(&&*features[index + 5].string))))
    {
        return;
    }
    for (offset, reading) in ["ム", "サ", "シ"].into_iter().enumerate() {
        let node = &mut features[index + offset];
        node.read = reading.to_owned();
        node.pron = reading.to_owned();
        node.mora_size = 1;
        node.acc = if offset == 0 { 1 } else { 0 };
        if offset > 0 {
            node.chain_flag = 1;
        }
    }
}

/// 「古い」が直接修飾する独立した「機」は、織機を表す「ハタ」と読む。
fn read_old_loom_hata(features: &mut [NjdFeature], index: usize) {
    let feature = &features[index];
    // 「ハタ」の生起コストを下げると「機を逃さず」の「キ」まで変わるため、修飾する形容詞で限定する
    // 「機械」「機会」などの複合語と、「輸送機」のような接尾辞の「機」は対象に入らない
    if feature.string == "機"
        && feature.pos == "名詞"
        && feature.pos_group1 != "接尾"
        && feature_at(features, index, -1).is_some_and(|f| f.orig == "古い")
    {
        set_reading(features, index, "ハタ", None);
        // 「ハタ」は2モーラの尾高型なので、独立したアクセント句では核を2に設定する
        if features[index].chain_flag != 1 {
            set_accent(features, index, 2);
        }
    }
}

/// 「守備の要として」「組織の要となる」のように、名詞と「の」に続いて「と」で受ける「要」は、
/// 中心となる部分を表す「カナメ」と読む。
fn read_central_kaname(features: &mut [NjdFeature], index: usize) {
    // 「説明の要はない」「刷新の要は、今や」のように「は」で受ける形は必要を表す「ヨウ」にもなる
    // ため、「と」が続く形に限る
    if features[index].string == "要"
        && features[index].pos == "名詞"
        && feature_at(features, index, -1).is_some_and(|f| f.string == "の" && f.pos == "助詞")
        && feature_at(features, index, -2).is_some_and(|f| f.pos == "名詞")
        && string_at_in(features, index, 1, &["と"])
    {
        set_reading(features, index, "カナメ", None);
        // 「カナメ」は平板型なので、独立したアクセント句では核を0に設定する
        if features[index].chain_flag != 1 {
            set_accent(features, index, 0);
        }
    }
}

/// 「歌の節」「唄の節」のように歌の旋律を指す「節」は、「フシ」と読む。
fn read_melodic_fushi(features: &mut [NjdFeature], index: usize) {
    // 「テーマ曲の節を参照」のように文書の区切りを指す「セツ」もあるため、前の名詞を「歌」「唄」に
    // 限る
    if features[index].string == "節"
        && features[index].pos == "名詞"
        && string_at_in(features, index, -1, &["の"])
        && string_at_in(features, index, -2, &["歌", "唄"])
    {
        set_reading(features, index, "フシ", None);
        // 「フシ」は2モーラの尾高型なので、独立したアクセント句では核を2に設定する
        if features[index].chain_flag != 1 {
            set_accent(features, index, 2);
        }
    }
}

/// 「痛みに堪える」「苦難にも堪えて」のように、苦痛を表す名詞を「に」で受ける「堪える」は、
/// 辛抱する意味の「タエル」と読む。
fn read_enduring_taeru(features: &mut [NjdFeature], index: usize) {
    let feature = &features[index];
    // 「笑いを堪える」の「コラエル」は「を」で受けるため、この形に入らない
    // 「寒さに体が慣れず、冷え込みが堪えた」の「コタエル」と区別するため、「に」と動詞の間には
    // 「も」と副詞だけを認める
    // 寒さ・暑さは「身にこたえる」の意味でも「に」で受けるため、名詞には含めない
    if !(feature.orig == "堪える"
        && feature.pos == "動詞"
        && (feature.read.starts_with("コタエ") || feature.read.starts_with("コラエ")))
    {
        return;
    }
    let cursor = features[..index]
        .iter()
        .rposition(|f| !(f.pos == "副詞" || (f.string == "も" && f.pos == "助詞")));
    if let Some(cursor) = cursor
        && cursor > 0
        && features[cursor].string == "に"
        && features[cursor].pos == "助詞"
        && ["痛み", "苦しみ", "苦痛", "苦難"].contains(&&*features[cursor - 1].orig)
    {
        let original_accent = feature.acc;
        let reading = format!("タエ{}", skip_chars(&feature.read, 3));
        set_reading(features, index, &reading, None);
        // 辞書の「コタエ」「コラエ」と「タエ」は、どれも語末から同じ位置に核がある（「3/3」と
        // 「2/2」）
        // NJDは活用形と後続の助動詞（「堪えた」の「タ＼エタ」、「堪えます」の「タエマ＼ス」）に
        // 合わせて核を決めるので、先頭の1モーラが減った分だけ核を前へずらす
        if features[index].chain_flag != 1 && original_accent > 0 {
            features[index].acc = original_accent - 1;
        }
    }
}

/// 「この社は神を祀る」のように、神社を指して「この」「その」「あの」で受ける独立した「社」は、
/// 「ヤシロ」と読む。
fn read_shrine_yashiro(features: &mut [NjdFeature], index: usize) {
    // 会社を指す「この社は」と同じ形になるため、同じ文に祭祀の語があり、会社の活動を示す語がない
    // 場合に限る
    // 「その社史」「その社号」のような複合語の一部と区別するため、直後が助詞の場合に限る
    // 「歴史」は会社の沿革にも使うので、祭祀の語には含めない
    if features[index].string == "社"
        && features[index].pos == "名詞"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.pos == "連体詞" && ["この", "その", "あの"].contains(&&*f.string))
        && feature_at(features, index, 1).is_some_and(|f| f.pos == "助詞")
        && sentence_has_any(
            features,
            index,
            &["神社", "社殿", "鳥居", "祭祀", "祀る", "神", "屋根神"],
        )
        && !sentence_has_any(
            features,
            index,
            &[
                "会社", "企業", "創業", "社員", "新聞", "出版", "記者", "営業", "売上",
            ],
        )
    {
        set_reading(features, index, "ヤシロ", None);
    }
}

/// 「汚れを祓う」のように、祓う動作の対象になる名詞の「汚れ」は、「ヨゴレ」でなく「ケガレ」と
/// 読む。
fn read_impurity_kegare(features: &mut [NjdFeature], index: usize) {
    // 「靴の汚れを清める」「物の汚れを除き清める」のように「清める」は物に付いた汚れにも使うため、
    // 「祓う」が「を」を挟んで直後に続く形に限る
    if features[index].string == "汚れ"
        && features[index].pos == "名詞"
        && feature_at(features, index, 1).is_some_and(|f| f.string == "を" && f.pos == "助詞")
        && feature_at(features, index, 2).is_some_and(|f| f.orig == "祓う")
    {
        set_reading(features, index, "ケガレ", None);
        // 「ケガレ」は尾高型と平板型の両方で読まれるので、独立したアクセント句では平板型の核0を
        // 設定する
        if features[index].chain_flag != 1 {
            set_accent(features, index, 0);
        }
    }
}

/// 訓練や苦痛に耐えられず弱音を吐く「音を上げる」の「音」は、「オト」でなく「ネ」と読む。
fn read_giving_up_ne(features: &mut [NjdFeature], index: usize) {
    // 音量を大きくする「音を上げる」と同じ形になるため、同じ文に苦痛を表す語があり、音響を表す語が
    // ない場合に限る
    if features[index].string == "音"
        && features[index].pos == "名詞"
        && string_at_in(features, index, 1, &["を"])
        && feature_at(features, index, 2).is_some_and(|f| f.orig == "上げる")
        && sentence_has_any(
            features,
            index,
            &[
                "訓練",
                "厳しい",
                "苦しい",
                "苦痛",
                "疲れる",
                "疲れ",
                "限界",
                "寒さ",
                "暑さ",
                "辛い",
            ],
        )
        && !sentence_has_any(
            features,
            index,
            &[
                "音量",
                "ボリューム",
                "スピーカー",
                "録画",
                "音楽",
                "雑音",
                "聞こえる",
                "鳴る",
                "イヤホン",
                "オーディオ",
                "テレビ",
                "ラジオ",
                "再生",
                "大音量",
                "サウンド",
                "BGM",
            ],
        )
    {
        set_reading(features, index, "ネ", None);
        // 「ネ」は平板型なので、独立したアクセント句では核を0に設定する
        if features[index].chain_flag != 1 {
            set_accent(features, index, 0);
        }
    }
}

/// 「紅色の着物」「薄紅色のドレス」のように衣服や布の色を指す「紅色」は、「コウショク」でなく
/// 「ベニイロ」と読む。
fn read_cloth_beniiro(features: &mut [NjdFeature], index: usize) {
    // 「紅色細菌」のような専門語は「コウショク」と読むので、辞書のコストは変えず、「の」に続く
    // 衣服や布の名詞で限定する
    // 「紅色の帯模様」は生き物の模様にも使うため、「帯」は対象に含めない
    if !(features[index].string == "紅色"
        && string_at_in(features, index, 1, &["の"])
        && string_at_in(
            features,
            index,
            2,
            &["敷物", "着物", "布", "衣", "袴", "旗", "衣装", "ドレス"],
        ))
    {
        return;
    }
    set_reading(features, index, "ベニイロ", None);
    // 「ベニイロ」は平板型なので、独立したアクセント句では核を0に設定する
    if features[index].chain_flag != 1 {
        set_accent(features, index, 0);
    }
    // 接頭辞「薄」と結合した「ウスベニイロ」も平板型なので、「コウショク」の結合で句の先頭に付いた
    // 核を外す
    else if index > 0
        && features[index - 1].string == "薄"
        && features[index - 1].pos == "接頭詞"
        && features[index - 1].chain_flag != 1
    {
        features[index - 1].acc = 0;
    }
}

/// 「盲・聾・養護学校」のように福祉・教育の語と列挙された「盲」「聾」を、音読みの「モウ」「ロウ」
/// と読む。
fn read_disability_enumeration(features: &mut [NjdFeature], index: usize) {
    let is_blind = features[index].string == "盲";
    // 単漢字の音読みをコストで優先すると文学作品の訓読みも変わるため、福祉・教育の語との列挙だけを
    // 補正する
    if ["盲", "聾"].contains(&&*features[index].string)
        && features[index].pos == "名詞"
        && is_disability_enumeration(features, index)
    {
        set_reading(
            features,
            index,
            if is_blind { "モウ" } else { "ロウ" },
            Some(if is_blind { "モー" } else { "ロー" }),
        );
        // 「モー」「ロー」は2モーラの頭高型なので、元の訓読みの核を引き継がずに設定する
        set_accent(features, index, 1);
    }
}

/// 「盲」「聾」が、中黒または読点を挟んで学校種別や障害種別と並んでいるかを判定する。
///
/// 「知的」「障害」のように複数の形態素に分かれた語も照合し、句点や無関係な語で隔てられた文脈は
/// 対象外とする。
fn is_disability_enumeration(features: &[NjdFeature], index: usize) -> bool {
    for direction in [-1, 1] {
        if !feature_at(features, index, direction)
            .is_some_and(|f| f.pos == "記号" && ["・", "、"].contains(&&*f.string))
        {
            continue;
        }
        let mut offset = 2 * direction;
        let mut term = String::new();
        while let Some(node) = feature_at(features, index, offset)
            && node.pos == "名詞"
        {
            if direction < 0 {
                term.insert_str(0, &node.string);
            } else {
                term.push_str(&node.string);
            }
            if DISABILITY_ENUMERATION_TERMS.contains(&&*term) {
                return true;
            }
            if !DISABILITY_ENUMERATION_TERMS.iter().any(|candidate| {
                if direction < 0 {
                    candidate.ends_with(&*term)
                } else {
                    candidate.starts_with(&*term)
                }
            }) {
                break;
            }
            offset += direction;
        }
    }
    false
}

/// 最上級を表す「一番」に続く形容動詞の「好き」を、接尾辞の「ズキ」でなく「スキ」と読む。
fn read_ichiban_suki(features: &mut [NjdFeature], index: usize) {
    let feature = &features[index];
    // 「好き」の行のコストを下げると「旅好き」も変わるため、最上級を表す「一番」に続く形容動詞だけを
    // 補正する
    if feature.string == "好き"
        && feature.pos_group1 == "接尾"
        && feature.pos_group2 == "形容動詞語幹"
        && string_at_in(features, index, -1, &["一番"])
    {
        set_reading(features, index, "スキ", None);
    }
}

/// 「何にも知らない」のように打ち消しの述語と組む「何にも」の「何」を、「ナン」と読む。
fn read_nannimo(features: &mut [NjdFeature], index: usize) -> bool {
    let feature = &features[index];
    // 「何にも」の副詞の行を優先すると「何にも依存しない」も変わるため、打ち消しの述語で読みを選ぶ
    if feature.string == "何"
        && feature.pos == "名詞"
        && index + 2 < features.len()
        && features[index + 1].string == "に"
        && features[index + 1].pos_group1 == "格助詞"
        && features[index + 2].string == "も"
        && features[index + 2].pos_group1 == "係助詞"
        && is_negative_nannimo_context(features, index + 3)
    {
        set_reading(features, index, "ナン", None);
        return true;
    }
    false
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

/// 恥じる動作や号令、古風な命令形「上げよ」で顔を表す「面を伏せる」「面を上げる」の「面」を、
/// 「オモテ」と読む。
fn read_face_omote(features: &mut [NjdFeature], index: usize) -> bool {
    let feature = &features[index];
    let previous = feature_at(features, index, -1);
    // 「面を伏せる」はラケットにも使うため、恥じる動作や号令、古風な命令形「上げよ」で顔を表す
    // 場合だけを補正する
    // 接尾辞の「面」と連体修飾を受ける「面」は表面も表すため、独立した名詞か、程度を表す
    // 「〜のあまり」の直後に限る
    if !(feature.string == "面"
        && feature.pos == "名詞"
        && ((feature.pos_group1 == "一般" && feature.chain_flag != 1)
            || (feature.pos_group1 == "接尾"
                && previous.is_some_and(|f| f.string == "あまり")
                && feature_at(features, index, -2).is_some_and(|f| f.pos_group1 == "連体化")))
        && previous.is_none_or(|f| f.pos_group1 != "連体化")
        && feature_at(features, index, 1)
            .is_some_and(|f| f.string == "を" && f.pos_group1 == "格助詞")
        && feature_at(features, index, 2)
            .is_some_and(|f| f.pos == "動詞" && ["伏せる", "上げる"].contains(&&*f.orig)))
    {
        return false;
    }

    let predicate = &features[index + 2];
    let mut is_face_movement = predicate.orig == "上げる" && predicate.cform == "命令ｙｏ";
    // 恥じる気持ちや号令は読点を挟んで動作につながるので、句点やかぎ括弧までの前方を調べる
    // 別の文の感情や命令が、表面を動かす説明に及ばないよう、文の境界で区切る
    for preceding in features[..index].iter().rev() {
        if ["。", "！", "？", "!", "?", "「", "」"].contains(&&*preceding.string) {
            break;
        }
        if ["恥ずかしい", "恥じる", "恥じらう", "照れる", "号令"].contains(&&*preceding.orig)
        {
            is_face_movement = true;
            break;
        }
    }
    if is_face_movement {
        // 「〜のあまり」に「面」が接尾辞として誤結合した場合は、顔を表す名詞を別のアクセント句にする
        if features[index].pos_group1 == "接尾" {
            features[index].chain_flag = 0;
        }
        set_reading(features, index, "オモテ", None);
        // 「オモテ」は3モーラの尾高型なので、「メン」「ツラ」のアクセント核を引き継がずに設定する
        set_accent(features, index, 3);
    }
    true
}

/// 船・舟・ボートが主語の「覆っ」を、覆い隠す「オオッ」でなく転覆を表す「クツガエッ」と読む。
fn read_capsized_kutsugae(features: &mut [NjdFeature], index: usize) -> bool {
    let feature = &features[index];
    // 「覆っ」のコストで「覆る」を優先すると雲や布が覆う文も変わるため、船・舟・ボートが主語の転覆
    // だけを補正する
    // 「覆う」「覆われる」「覆い尽くす」は覆い隠す意味でも使うので、活用形が重なる「覆っ」に限定する
    if !(feature.string == "覆っ"
        && feature.pos == "動詞"
        && feature.orig == "覆う"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.string == "が" && f.pos_group1 == "格助詞")
        && feature_at(features, index, -2)
            .is_some_and(|f| ["船", "舟", "ボート"].contains(&&*f.string) && f.pos == "名詞"))
    {
        return false;
    }

    let mut is_covering = false;
    // 「海面を船が覆った」のような語順も他動詞なので、同じ節の主語より前にある目的語を調べる
    // 動詞や文の境界を越えた目的語は別の述語に属するため、そこで探索を区切る
    for preceding in features[..index - 2].iter().rev() {
        if ["動詞", "形容詞"].contains(&&*preceding.pos)
            || ["。", "！", "？", "!", "?", "「", "」"].contains(&&*preceding.string)
        {
            break;
        }
        if preceding.string == "を" && preceding.pos_group1 == "格助詞" {
            is_covering = true;
            break;
        }
    }
    // 「船が覆っていた海面」は海面を覆う連体修飾にもなるため、助動詞や補助動詞の後に自立した名詞が
    // ある場合は既定の読みを保つ
    // 「覆ったこと」のように出来事を名詞化する形は、転覆を表す用法にもなるので対象に残す
    // 「覆って乗員が落ちた」の「て」は次の節をつなぐので、非自立の動詞が続く場合だけ同じ述語の
    // 一部としてたどる
    let mut ending = index + 1;
    while ending < features.len() {
        let node = &features[ending];
        if !(node.pos == "助動詞"
            || (node.pos == "動詞" && node.pos_group1 == "非自立")
            || (node.string == "て"
                && node.pos_group1 == "接続助詞"
                && feature_at(features, ending, 1)
                    .is_some_and(|f| f.pos == "動詞" && f.pos_group1 == "非自立")))
        {
            break;
        }
        ending += 1;
    }
    if features
        .get(ending)
        .is_some_and(|f| f.pos == "名詞" && f.pos_group1 != "非自立")
    {
        is_covering = true;
    }
    if !is_covering {
        set_reading(features, index, "クツガエッ", None);
        // 自動詞「覆る」は「ガ」の後に下がるため、原形と活用型も「覆る」に合わせ、アクセント核を3に
        // 設定する
        let feature = &mut features[index];
        feature.orig = "覆る".to_owned();
        feature.ctype = "五段・ラ行".to_owned();
        feature.acc = 3;
    }
    true
}

/// 崖・淵・水の端を表す「〜の際」の「際」を、時や場合を表す「サイ」でなく「キワ」と読む。
fn read_edge_kiwa(features: &mut [NjdFeature], index: usize) -> bool {
    // 「キワ」の生起コストを下げると「その際」「出発の際」の読みも変わるため、崖・淵・水の端を表す
    // 「〜の際」だけを補正する
    // 「洪水の際」「排水の際」は時や場合を表すので、「の」の前が独立した名詞「崖」「淵」「水」である
    // 場合に限定する
    if !(features[index].string == "際"
        && features[index].pos == "名詞"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.string == "の" && f.pos_group1 == "連体化")
        && feature_at(features, index, -2)
            .is_some_and(|f| ["崖", "淵", "水"].contains(&&*f.string) && f.pos == "名詞"))
    {
        return false;
    }

    // 非自立名詞「サイ」の結合で前のアクセント句に付いた核を外し、「淵の」など前の語にある核は保つ
    if features[index].chain_flag == 1 {
        let mut head = index - 1;
        while head > 0 && features[head].chain_flag == 1 {
            head -= 1;
        }
        let preceding_mora_size = features[head..index]
            .iter()
            .map(|f| f.mora_size)
            .sum::<i32>();
        if features[head].acc > preceding_mora_size {
            features[head].acc = 0;
        }
    }
    // 端を表す「キワ」は一般名詞なので、時や場合を表す「サイ」と区別して別のアクセント句で尾高型に
    // する
    set_reading(features, index, "キワ", None);
    let feature = &mut features[index];
    feature.pos_group1 = "一般".to_owned();
    feature.pos_group2 = "*".to_owned();
    feature.chain_rule = "C3".to_owned();
    feature.chain_flag = 0;
    set_accent(features, index, 2);
    true
}

/// 法要で唱える「声明」を、発表の「セイメイ」でなく「ショウミョウ」と読む。
fn read_chanted_shomyo(features: &mut [NjdFeature], index: usize) -> bool {
    // 「声明」は政治的な発表にも使われるため、「声明を唱える」と受身の「声明が唱えられる」だけを
    // 補正する
    // 宗教団体の政治的な声明も「セイメイ」と読むので、文中の宗教語の有無では判定せず、後続する動詞の
    // 原形が「唱える」かどうかで判定する
    // 「声明が唱える理念」は声明文自体が主語になるので、「が」の後は「られる」が続く受身形に限定する
    if !(features[index].string == "声明"
        && features[index].pos == "名詞"
        && feature_at(features, index, 1)
            .is_some_and(|f| ["を", "が"].contains(&&*f.string) && f.pos_group1 == "格助詞")
        && feature_at(features, index, 2).is_some_and(|f| f.pos == "動詞" && f.orig == "唱える")
        && (features[index + 1].string == "を"
            || feature_at(features, index, 3)
                .is_some_and(|f| f.orig == "られる" && f.pos_group1 == "接尾")))
    {
        return false;
    }

    set_reading(features, index, "ショウミョウ", Some("ショーミョー"));
    // 平板の「セイメイ」から頭高型の「ショーミョー」へ変わるため、アクセント核を1に設定する
    set_accent(features, index, 1);
    true
}

/// 材料の「麻布」を、地名の「アザブ」でなく「アサヌノ」と読む。
fn read_hemp_cloth_asanuno(features: &mut [NjdFeature], index: usize) -> bool {
    // 「麻布」は地名にも使われるため、生成り・織りの修飾や、染色・製織・漆加工の対象となる局所的な
    // 関係だけを補正する
    // 文中に織物や染色の語があるだけでは、教室の所在地なども変わるので、助詞を挟んだ直前・直後の
    // 関係に限定する
    if !(features[index].string == "麻布" && features[index].pos == "名詞") {
        return false;
    }

    let is_cloth_modifier = feature_at(features, index, -1)
        .is_some_and(|f| f.string == "の" && f.pos_group1 == "連体化")
        && string_at_in(features, index, -2, &["生成り", "織り"]);
    let mut particle_index = index + 1;
    if string_at_in(features, index, 1, &["など"]) {
        particle_index += 1;
    }
    let mut is_cloth_processing = false;
    if particle_index + 1 < features.len()
        && features[particle_index].pos == "助詞"
        && ["を", "は"].contains(&&*features[particle_index].string)
    {
        let predicate = &features[particle_index + 1];
        let after_predicate = features.get(particle_index + 2);
        is_cloth_processing = (predicate.pos == "動詞"
            && ["染める", "織る"].contains(&&*predicate.orig))
            || (predicate.string == "染色"
                && predicate.pos_group1 == "サ変接続"
                && after_predicate.is_some_and(|f| f.orig == "する" && f.pos == "動詞"))
            || (predicate.string == "漆"
                && predicate.pos == "名詞"
                && after_predicate.is_some_and(|f| f.string == "で" && f.pos_group1 == "格助詞"));
    }
    if is_cloth_modifier || is_cloth_processing {
        // 「アサヌノ」は平板なので、元の語の中にある核は外し、「など」に由来する後続の核はモーラ数の
        // 変更に合わせて保つ
        let old_mora_size = features[index].mora_size;
        set_reading(features, index, "アサヌノ", None);
        if features[index].acc <= old_mora_size {
            set_accent(features, index, 0);
        }
    }
    true
}

/// 「二」「十」を、まとめて「ハタ」「チ」と読み替える。
///
/// 合計は3モーラのままなので、アクセント句の区切りを保つ。
fn read_hatachi(features: &mut [NjdFeature], index: usize) {
    let two = &mut features[index];
    two.read = "ハタ".to_owned();
    two.pron = "ハタ".to_owned();
    two.mora_size = 2;
    let ten = &mut features[index + 1];
    ten.read = "チ".to_owned();
    ten.pron = "チ".to_owned();
    ten.mora_size = 1;
}

/// 年齢を表す「二十前」の「二十」を、数の「ニジュウ」でなく「ハタチ」と読む。
fn read_age_before_twenty(features: &mut [NjdFeature], index: usize) -> bool {
    // 「二十前」は年齢を表す用法が多いため、数詞の「二」「十」に接尾辞「前」が続く形を「ハタチ」に
    // する
    // 「二十前半」「二十前後」「二十時前」と「百二十」などの末尾の「二十」は、形態素の区切りで区別
    // して数詞の読みを保つ
    if !(features[index].string == "二"
        && features[index].pos_group1 == "数"
        && feature_at(features, index, 1)
            .is_some_and(|f| f.string == "十" && f.pos_group1 == "数" && f.chain_flag == 1)
        && feature_at(features, index, 2).is_some_and(|f| {
            f.string == "前" && f.pos_group1 == "接尾" && f.read == "マエ" && f.chain_flag == 1
        })
        && feature_at(features, index, -1).is_none_or(|f| f.pos_group1 != "数")
        && feature_at(features, index, 3)
            .is_none_or(|f| !(f.string.starts_with('半') || f.string.starts_with('後'))))
    {
        return false;
    }

    // 片方だけを変えると数詞の読みが混ざるため、両方の読みをまとめて変更する
    // 合計は3モーラのままなので、「前」のアクセント核とアクセント句の区切りを保つ
    read_hatachi(features, index);
    true
}

/// 単位を省いた年齢を表す「二十を過ぎる」の「二十」を、数の「ニジュウ」でなく「ハタチ」と読む。
fn read_age_past_twenty(features: &mut [NjdFeature], index: usize) -> bool {
    // 「二十」の語を優先すると「二十を数える」も「ハタチ」になるため、単位を省いた「二十を過ぎる」に
    // 絞る
    // 同じ節に回数・得点・時刻などがある場合は数量とし、前に数詞がある「百二十」も補正しない
    if !(features[index].string == "二"
        && features[index].pos_group1 == "数"
        && feature_at(features, index, 1)
            .is_some_and(|f| f.string == "十" && f.pos_group1 == "数" && f.chain_flag == 1)
        && index + 3 < features.len()
        && features[index + 2].string == "を"
        && features[index + 3].orig == "過ぎる"
        && feature_at(features, index, -1).is_none_or(|f| f.pos_group1 != "数")
        && is_age_past_twenty_context(features, index))
    {
        return false;
    }

    // 合計3モーラと形態素数を保ち、両方の読みをまとめて変更する
    read_hatachi(features, index);
    features[index].acc = 1;
    true
}

/// 単位を省いた「二十」の前方を同じ節の中で調べ、年齢として補正できるかを判定する。
///
/// 年齢を表す名詞が先に見つかるか、数量や時刻を表す名詞がない場合は`true`。
fn is_age_past_twenty_context(features: &[NjdFeature], start: usize) -> bool {
    // 「5：20」のコロンは節の境界にもなるため、前が数詞なら時刻の数の並びとして先に除外する
    if start >= 2
        && [":", "："].contains(&&*features[start - 1].string)
        && features[start - 2].pos_group1 == "数"
    {
        return false;
    }

    for node in features[..start].iter().rev() {
        if node.pos == "記号" {
            break;
        }
        if node.pos != "名詞" {
            continue;
        }
        if ["年齢", "齢", "歳", "才"].contains(&&*node.string) {
            return true;
        }
        if [
            "回数",
            "得点",
            "点数",
            "スコア",
            "数",
            "人数",
            "個数",
            "数量",
            "件数",
            "時刻",
            "時間",
            "分",
            "秒",
            "カウント",
            "打数",
            "失点",
            "年数",
            "番号",
            "ページ",
        ]
        .contains(&&*node.string)
        {
            return false;
        }
    }
    true
}

/// 「八つ、九つ、十」のように和語の数詞で数え上げた後の「十」を、「ジュウ」でなく「トオ」と読む。
fn read_counted_too(features: &mut [NjdFeature], index: usize) -> bool {
    // 「十」に助数詞や数が続く場合は、漢語の数詞としてNJDの数詞処理の読みを保つ
    if features[index].string == "十"
        && features[index].pos_group1 == "数"
        && (string_at_in(features, index, -1, &["九つ"])
            || (string_at_in(features, index, -1, &["、"])
                && string_at_in(features, index, -2, &["九つ"])))
        && feature_at(features, index, 1)
            .is_none_or(|f| f.pos_group1 != "数" && f.pos_group2 != "助数詞")
    {
        set_reading(features, index, "トオ", Some("トー"));
        return true;
    }
    false
}

/// 算用数字の「1日中」の「中」を、辞書の1語の「一日中」と同じく「チュウ」でなく「ジュウ」と読む。
fn read_all_day_juu(features: &mut [NjdFeature], index: usize) -> bool {
    // NJDは「1日」を日数の1語「一日」（「イチニチ」）にまとめるので、その直後の「中」に限る
    // 「十一日」のような長い数の末尾の「一日」と、「2日中に」の日付の「フツカ」や「ツイタチ」に続く
    // 「中」は、「ジュウ」と決められないので変えない
    if features[index].string == "中"
        && features[index].read == "チュウ"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.string == "一日" && f.read == "イチニチ")
        && feature_at(features, index, -2).is_none_or(|f| f.pos_group1 != "数")
    {
        set_reading(features, index, "ジュウ", Some("ジュー"));
        return true;
    }
    false
}

/// 鋳型の「金型」の「金」を「カナ」、証書の「金五万円」のように金額の前に付く「金」を「キン」と
/// 読む。
fn read_kin(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string != "金" {
        return false;
    }
    let previous = feature_at(features, index, -1);
    let following = feature_at(features, index, 1);
    // 「月〜金」の曜日の範囲は、金銭の「カネ」と区別して「キン」と読む
    // 空白や営業時間の後続によって最良候補が変わるため、範囲の両端が曜日の字である場合に限る
    if previous.is_some_and(|f| ["〜", "～", "ー", "－", "−"].contains(&&*f.string))
        && string_at_in(features, index, -2, &["月", "火", "水", "木", "土", "日"])
    {
        set_reading(features, index, "キン", None);
        return true;
    }
    // 「返戻金型」は「返戻金」に「型」が付く表現なので、鋳型の「金型」と分けて「キン」を保つ
    if following.is_some_and(|f| f.string == "型") && previous.is_none_or(|f| f.string != "返戻")
    {
        set_reading(features, index, "カナ", None);
        return true;
    }
    // 証書の「金五万円」「金拾万円」のように金額の前に付く「金」は、金銭の「カネ」でなく「キン」と
    // 読む
    // 「金」の「キン」と「カネ」は同じ名詞でコストが近く、直後の数詞だけでは辞書の連接で決まらない
    // 大字の「壱」「参」「伍」は辞書で数でない名詞として解析されるため、表層で数詞と見なす
    if following.is_some_and(|f| {
        f.pos == "名詞" && (f.pos_group1 == "数" || ["壱", "参", "伍"].contains(&&*f.string))
    }) {
        set_reading(features, index, "キン", None);
        return true;
    }
    false
}

/// 「就学前の児童」のようにサ変接続の名詞と助詞に挟まれた「前」を、句として「マエ」と読む。
fn read_mae_after_sahen_noun(features: &mut [NjdFeature], index: usize) -> bool {
    // 「就学前の児童」は句として「マエ」、「就学前教育」は複合語として「ゼン」と読む
    // 「就学」は「ゼン」と読ませる前接語にも含まれるので、この規則を前接語の表より先に試す
    if features[index].string == "前"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.pos == "名詞" && f.pos_group1 == "サ変接続")
        && feature_at(features, index, 1).is_some_and(|f| f.pos == "助詞")
    {
        set_reading(features, index, "マエ", None);
        return true;
    }
    false
}

/// 直前か直後の語の表層形だけで読みが決まる語を、[`ADJACENT_WORD_READINGS`]の表から引いて読みを
/// 書き換える。
fn read_by_adjacent_word(features: &mut [NjdFeature], index: usize) -> bool {
    let feature = &features[index];
    let found = ADJACENT_WORD_READINGS.iter().find(
        |&&(surface, pos_group1, previous_words, following_words, _, _)| {
            surface == feature.string
                && pos_group1.is_none_or(|group| feature.pos_group1 == group)
                && previous_words.is_none_or(|words| string_at_in(features, index, -1, words))
                && following_words.is_none_or(|words| string_at_in(features, index, 1, words))
        },
    );
    if let Some(&(_, _, _, _, reading, pronunciation)) = found {
        set_reading(features, index, reading, pronunciation);
        return true;
    }
    false
}

/// フランス紙「ルモンド」や都市名「マルセイユ」の後に括弧で添えた「仏」は、国名の略称「フツ」と
/// 読む。
fn read_futsu_parenthetical_country(features: &mut [NjdFeature], index: usize) -> bool {
    // 国名の候補をコストだけで優先すると、仏教の「本国の仏」の「ホトケ」まで「フツ」へ変わる
    // フランスの新聞名・都市名が略称の意味を確定する場合に限定し、仏教の文脈での読みを保つ
    if features[index].string == "仏"
        && string_at_in(features, index, -1, &["（", "("])
        && string_at_in(features, index, -2, &["ルモンド", "マルセイユ"])
        && string_at_in(features, index, 1, &["）", ")"])
    {
        set_reading(features, index, "フツ", None);
        set_accent(features, index, 1);
        return true;
    }
    false
}

/// 氏名の後に括弧で添えた「（仮名）」は、本名に代わる名前を表す「カメイ」と読む。
fn read_kamei_after_person_name(features: &mut [NjdFeature], index: usize) -> bool {
    // 括弧だけで判定すると、「自動車登録番号(仮名)」のように文字種を表す「カナ」まで変わるため、
    // 直前が人名の場合に限る
    if features[index].string == "仮名"
        && string_at_in(features, index, -1, &["（", "("])
        && feature_at(features, index, -2).is_some_and(|f| f.pos_group2 == "人名")
        && string_at_in(features, index, 1, &["）", ")"])
    {
        set_reading(features, index, "カメイ", Some("カメー"));
        return true;
    }
    false
}

/// 助成を行う行政主体として文頭に立つ「都」は「ト」と読み、都市を表す「ミヤコ」と区別する。
fn read_to_administration(features: &mut [NjdFeature], index: usize) -> bool {
    const SENTENCE_ENDS: &[&str] = &["。", "！", "？", "!", "?"];
    // 同品詞の「ト」を一律に優先すると「京の都」「都ホテル」も変わるため、行政の述語がある文頭の
    // 主語に限る
    if !(features[index].string == "都"
        && feature_at(features, index, -1).is_none_or(|f| SENTENCE_ENDS.contains(&&*f.string))
        && string_at_in(features, index, 1, &["は"]))
    {
        return false;
    }
    for node in &features[index + 2..] {
        if SENTENCE_ENDS.contains(&&*node.string) {
            break;
        }
        if node.orig == "助成" {
            set_reading(features, index, "ト", None);
            set_accent(features, index, 1);
            return true;
        }
    }
    false
}

/// 「届きます様に」「言う様な」のように述語に続く助動詞「ようだ」の用法の「様」を、敬称の「サマ」
/// でなく「ヨウ」と読む。
fn read_you_auxiliary(features: &mut [NjdFeature], index: usize) -> bool {
    // 「ヨウ」のコストだけを下げると「帰る様を描く」の「サマ」まで変わるため、「に」「な」が続く形に
    // 絞る
    // 動作の様子を指す「描く様には驚く」も同じ形になるが、助動詞の用法を優先する
    if features[index].string == "様"
        && feature_at(features, index, -1).is_some_and(|f| {
            ["動詞", "形容詞", "助動詞"].contains(&&*f.pos)
                && ["基本形", "連体形"].contains(&&*f.cform)
        })
        && feature_at(features, index, 1).is_some_and(|f| {
            (f.string == "に" && f.pos == "助詞") || (f.string == "な" && f.pos == "助動詞")
        })
    {
        set_reading(features, index, "ヨウ", Some("ヨー"));
        return true;
    }
    false
}

/// 神々の時代を表す「神の代」の「代」を、代金や世代の「ダイ」でなく「ヨ」と読む。
fn read_kami_era_yo(features: &mut [NjdFeature], index: usize) -> bool {
    let feature = &features[index];
    // 「代」のコストだけを下げると代金などの「ダイ」も変わるため、神々の時代を表す「神の代」に絞る
    // 短い連語の登録は「代理」「代行」「代弁」にも届くため、独立した名詞の「代」と前後の品詞を
    // 確かめる
    if feature.string == "代"
        && feature.pos == "名詞"
        && feature.pos_group1 == "一般"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.string == "の" && f.pos == "助詞" && f.pos_group1 == "連体化")
        && feature_at(features, index, -2)
            .is_some_and(|f| f.string == "神" && f.pos == "名詞" && f.pos_group1 == "一般")
        && feature_at(features, index, 1)
            .is_none_or(|f| ["助詞", "助動詞", "記号"].contains(&&*f.pos))
    {
        set_reading(features, index, "ヨ", None);
        return true;
    }
    false
}

/// 「支配の下」「条件の下」のように抽象名詞と「の」に続く「下」を、「シタ」でなく「モト」と読む。
fn read_abstract_moto(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string == "下"
        && features[index].pos_group1 == "一般"
        && string_at_in(features, index, -1, &["の"])
        && string_at_in(features, index, -2, ABSTRACT_NO_PREDECESSORS)
    {
        set_reading(features, index, "モト", None);
        return true;
    }
    false
}

/// 「下四桁」「上二桁」のように数字の桁の位置を指す「下」「上」を、「シタ」「ウエ」でなく
/// 「シモ」「カミ」と読む。
fn read_digit_position(features: &mut [NjdFeature], index: usize) -> bool {
    let is_lower = features[index].string == "下";
    if ["下", "上"].contains(&&*features[index].string)
        && feature_at(features, index, 1).is_some_and(|f| f.pos_group1 == "数")
        && string_at_in(features, index, 2, &["桁"])
    {
        set_reading(
            features,
            index,
            if is_lower { "シモ" } else { "カミ" },
            None,
        );
        return true;
    }
    false
}

/// 名詞に続く接尾辞の「橋」を「バシ」と読む（構造種別の前接語に続く「キョウ」は
/// [`ADJACENT_WORD_READINGS`]の表で先に読む）。
fn read_bashi_suffix(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string == "橋"
        && features[index].pos_group1 == "接尾"
        && feature_at(features, index, -1).is_some_and(|f| f.pos == "名詞")
    {
        set_reading(features, index, "バシ", None);
        return true;
    }
    false
}

/// 2文字以上の自立語に続く「者」を、「シャ」でなく「モノ」と読む。
fn read_mono(features: &mut [NjdFeature], index: usize) -> bool {
    // 「受者」の「受」も動詞として解析されるため、1文字の語幹に続く「者」は辞書の読みを保つ
    if features[index].string == "者"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.pos_group1 == "自立" && f.string.chars().count() > 1)
    {
        set_reading(features, index, "モノ", None);
        return true;
    }
    false
}

/// 「走り入って」「攻め入って」のように複合動詞を作る「入っ」を、「ハイッ」でなく「イッ」と読む。
fn read_compound_iru(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string == "入っ"
        && feature_at(features, index, -1).is_some_and(|f| {
            f.pos == "動詞"
                && f.cform == "連用形"
                && IRU_COMPOUND_PREDECESSORS.contains(&&*f.string)
        })
    {
        set_reading(features, index, "イッ", None);
        return true;
    }
    false
}

/// 放置を意味する「放っておく」は「ホーッテ」と読み、「放つ」の活用形「ハナッタ」と区別する。
fn read_leave_houtte(features: &mut [NjdFeature], index: usize) -> bool {
    // 単語コストの調整だけでは、「ホームランを放った」と「放っておく」の両方の読みを保てない
    // 「矢を放って逃げる」は「ハナッテ」なので、補助動詞の「おく」と、その縮約形の「とく」が続く
    // 用法に限る
    if features[index].string != "放っ" {
        return false;
    }
    let Some(following) = feature_at(features, index, 1) else {
        return false;
    };
    if (following.string == "て"
        && feature_at(features, index, 2).is_some_and(|f| f.orig == "おく" && f.pos == "動詞"))
        || (following.orig == "とく" && following.pos == "動詞")
    {
        // アクセント核は、結合の前に`apply_original_rule_before_chaining`が平板型の「放る」に合わせて
        // 0にしている
        set_reading(features, index, "ホウッ", Some("ホーッ"));
        return true;
    }
    false
}

/// 「いつか公になる」のように副詞可能名詞に続く「公」を、「オーヤケ」と読む。
fn read_ooyake(features: &mut [NjdFeature], index: usize) -> bool {
    // 名詞の後の一律の補正（[`COMPOUND_SUFFIX_READINGS`]の「コウ」）から分けるため、その規則より
    // 先に試す
    if features[index].string == "公"
        && features[index].pos_group1 == "一般"
        && feature_at(features, index, -1).is_some_and(|f| f.pos_group1 == "副詞可能")
    {
        set_reading(features, index, "オオヤケ", Some("オーヤケ"));
        return true;
    }
    false
}

/// 「柄」は同じ名詞の候補に「ガラ」「エ」「ツカ」があるので、「の」の前の道具名で分ける。
fn read_handle_e_or_tsuka(features: &mut [NjdFeature], index: usize) -> bool {
    if !(features[index].string == "柄" && string_at_in(features, index, -1, &["の"])) {
        return false;
    }
    let Some(previous_previous) = feature_at(features, index, -2) else {
        return false;
    };
    if TOOL_HANDLE_PREDECESSORS.contains(&&*previous_previous.string) {
        set_reading(features, index, "エ", None);
    } else if SWORD_HILT_PREDECESSORS.contains(&&*previous_previous.string) {
        set_reading(features, index, "ツカ", None);
    }
    true
}

/// 器物や形状を表す名詞と「の」に続く「縁」を、「エン」でなく「フチ」と読む。
fn read_rim_fuchi(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string == "縁"
        && string_at_in(features, index, -1, &["の"])
        && string_at_in(features, index, -2, FUCHI_PREDECESSORS)
    {
        set_reading(features, index, "フチ", None);
        return true;
    }
    false
}

/// 名詞に続く「茶屋」を、連濁した「ジャヤ」と読む。
fn read_teahouse_jaya(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string == "茶屋" && previous_is_content_noun(features, index) {
        set_reading(features, index, "ジャヤ", None);
        return true;
    }
    false
}

/// 「芭蕉翁」のように名詞に続く「翁」を、「オキナ」でなく「オー」と読む。
fn read_honorific_ou(features: &mut [NjdFeature], index: usize) -> bool {
    // 「芭蕉翁」は「オー」と読むが、「明日翁が来る」のような独立用法は「オキナ」を保つ
    if features[index].string == "翁" && previous_is_content_noun(features, index) {
        set_reading(features, index, "オウ", Some("オー"));
        return true;
    }
    false
}

/// 直前が一般・固有名詞・サ変接続の名詞か。
fn previous_is_content_noun(features: &[NjdFeature], index: usize) -> bool {
    feature_at(features, index, -1).is_some_and(|f| {
        f.pos == "名詞" && ["一般", "固有名詞", "サ変接続"].contains(&&*f.pos_group1)
    })
}

/// 活用語に続く場所を表す「処」を、「ショ」でなく「トコロ」と読む。
fn read_tokoro(features: &mut [NjdFeature], index: usize) -> bool {
    // コストを下げても「処」が「ショ」に戻る文があるため、活用語に続く場所の「トコロ」は文脈で確定
    // する
    if features[index].string == "処"
        && feature_at(features, index, -1)
            .is_some_and(|f| f.pos_group1 == "自立" || f.pos == "助動詞")
    {
        set_reading(features, index, "トコロ", None);
        return true;
    }
    false
}

/// 形容詞の「強い」に「た」が付いた「強いた」を、動詞「強いる」の過去形として「シイ」と読む。
fn read_shiita(features: &mut [NjdFeature], index: usize) -> bool {
    // 形容詞の終止形に「た」は続かないので、MeCabが形容詞の「強い」に「た」を付けた「強いた」は、
    // 動詞「強いる」の過去形として「シイ」と読む
    // 動詞「強いる」の「シイ」の行のコストを下げると、「強い風」「強い国」の形容詞まで「シイ」になる
    // ので、後続の「た」で決める
    if features[index].string == "強い"
        && features[index].pos == "形容詞"
        && feature_at(features, index, 1).is_some_and(|f| f.pos == "助動詞" && f.string == "た")
    {
        set_reading(features, index, "シイ", None);
        return true;
    }
    false
}

/// 候文の「申し上げ候」のように動詞の連用形に続く「候」を、名詞の「コウ」でなく補助動詞の
/// 「ソウロウ」と読む。
fn read_sourou(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string == "候"
        && feature_at(features, index, -1).is_some_and(|f| f.pos == "動詞" && f.cform == "連用形")
    {
        set_reading(features, index, "ソウロウ", Some("ソーロー"));
        return true;
    }
    false
}

/// 「より外にない」は選択肢の「ホカ」、物体の位置を比べる場合は「ソト」と読む。
fn read_hoka(features: &mut [NjdFeature], index: usize) -> bool {
    if !(features[index].string == "外"
        && string_at_in(features, index, -1, &["より"])
        && feature_at(features, index, -2)
            .is_some_and(|f| f.pos == "動詞" || f.pos_group1 == "代名詞"))
    {
        return false;
    }

    for later in &features[index + 1..] {
        if ["。", "．", "！", "？"].contains(&&*later.string) {
            break;
        }
        if NEGATIVE_ORIGINALS.contains(&&*later.orig) {
            set_reading(features, index, "ホカ", None);
            break;
        }
    }
    true
}

/// 名詞へ直接続く後部要素は、助詞を挟んだ独立用法と区別して複合語の読みへ変える。
fn read_compound_suffix(features: &mut [NjdFeature], index: usize) -> bool {
    let feature = &features[index];
    let surface = &*feature.string;
    let Some(&(_, reading, pronunciation, required_pos_group1)) = COMPOUND_SUFFIX_READINGS
        .iter()
        .find(|(word, ..)| *word == surface)
    else {
        return false;
    };
    // 数詞に続く「部屋」は助数詞なので、NJDの数詞処理が選んだ「ヘヤ」を保つ
    // 日付に続く「付」は辞書の「ヅケ」を保つ
    // 「十日」は地名、「一日」「四日」は副詞可能名詞になるため、数字と「日」からなる表層形も日付
    // として扱う
    let Some(previous) = feature_at(features, index, -1) else {
        return false;
    };
    if !(previous.pos == "名詞"
        && (surface != "部屋" || previous.pos_group1 != "数")
        && !(surface == "付"
            && previous.string.ends_with('日')
            && (previous.pos_group2 == "助数詞"
                || (previous.string.chars().count() > 1
                    && previous.string.chars().rev().skip(1).all(|c| {
                        "一二三四五六七八九十〇零0123456789０１２３４５６７８９".contains(c)
                    })))))
    {
        return false;
    }

    if required_pos_group1.is_none_or(|group| feature.pos_group1 == group) {
        set_reading(features, index, reading, Some(pronunciation));
    }
    true
}

/// 「等」は代名詞に続けば「ラ」、自立した名詞に続けば「トウ」（「トー」）、活用語や形式名詞では
/// 既定の「ナド」を残す。
fn read_ra_or_tou(features: &mut [NjdFeature], index: usize) -> bool {
    if features[index].string != "等" {
        return false;
    }
    let Some(previous) = feature_at(features, index, -1) else {
        return false;
    };
    if previous.pos_group1 == "代名詞" {
        set_reading(features, index, "ラ", None);
        return true;
    }
    if ["一般", "サ変接続", "固有名詞", "接尾"].contains(&&*previous.pos_group1) {
        set_reading(features, index, "トウ", Some("トー"));
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use open_jtalk::NjdFeature;

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

    /// pyopenjtalkの`NJDFeature`の項目順に空白区切りで並べた項目からNJDノードを作る。
    ///
    /// [`parse_nodes`]と異なり、`pos_group2`と`pos_group3`も含む。
    fn parse_full_node(line: &str) -> NjdFeature {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let [
            string,
            pos,
            pos_group1,
            pos_group2,
            pos_group3,
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
            panic!("expected 14 fields: {line}");
        };
        NjdFeature {
            string: string.to_owned(),
            pos: pos.to_owned(),
            pos_group1: pos_group1.to_owned(),
            pos_group2: pos_group2.to_owned(),
            pos_group3: pos_group3.to_owned(),
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

    /// 改行区切りの[`parse_full_node`]。
    fn parse_full_nodes(lines: &str) -> Vec<NjdFeature> {
        lines
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(parse_full_node)
            .collect()
    }

    /// `use_vanilla=True`の`run_frontend`の結果に、pyopenjtalk-plusの`modify_context_reading`を
    /// 適用したときと同じ形態素を書き換える。
    ///
    /// 「守備の要として」「切れ味が鈍る」「服従を強いた」「より良い社会」「花弁などを摘む」は、
    /// 規則が働くよう辞書の読みや品詞を差し替えた入力。
    ///
    /// `changes`は書き換わる形態素の位置と、書き換えた後の[`parse_full_node`]の項目。
    #[rstest]
    // 髪を元結で結ぶ。
    #[case(
        "髪 名詞 一般 * * * * 髪 カミ カミ 2 2 C4 -1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         元結 名詞 一般 * * * * 元結 モットイ モットイ 3 4 C1 0
         で 助詞 格助詞 一般 * * * で デ デ 1 1 動詞%F1 1
         結ぶ 動詞 自立 * * 五段・バ行 基本形 結ぶ ムスブ ムスブ 0 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "元結 名詞 一般 * * * * 元結 モトユイ モトユイ 3 4 C1 0")],
    )]
    // 黒白をつけた
    #[case(
        "黒白 名詞 一般 * * * * 黒白 クロシロ クロシロ 1 4 C1 -1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         つけ 動詞 自立 * * 一段 連用形 つける ツケ ツケ 1 2 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1",
        &[(0, "黒白 名詞 一般 * * * * 黒白 コクビャク コクビャク 0 4 C1 -1")],
    )]
    // ワインに開眼です。
    #[case(
        "ワイン 名詞 一般 * * * * ワイン ワイン ワイン 1 3 C1 -1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         開眼 名詞 サ変接続 * * * * 開眼 カイガン カイガン 5 4 C2 0
         です 助動詞 * * * 特殊・デス 基本形 です デス デス’ 1 2 助動詞%F2@0/助詞%F2@0/連体詞%F2@0/名詞%F2@1/動詞%F1/形容詞%F2@0 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "開眼 名詞 サ変接続 * * * * 開眼 カイゲン カイゲン 5 4 C2 0")],
    )]
    // その他にも方法がある。
    #[case(
        "その他 名詞 代名詞 一般 * * * その他 ソノタ ソノタ 2 3 C1 -1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         も 助詞 係助詞 * * * * も モ モ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/副詞%F2@0/助詞%F2@0 1
         方法 名詞 一般 * * * * 方法 ホウホウ ホーホー 0 4 C2 0
         が 助詞 格助詞 一般 * * * が ガ ガ 0 1 名詞%F1 1
         ある 動詞 自立 * * 五段・ラ行 基本形 ある アル アル 1 2 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "その他 名詞 代名詞 一般 * * * その他 ソノホカ ソノホカ 2 4 C1 -1")],
    )]
    // より良い社会
    #[case(
        "より 副詞 一般 * * * * より ヨリ ヨリ 0 2 * -1
         良い 形容詞 自立 * * 形容詞・アウオ段 基本形 良い イイ イイ 1 2 * 0
         社会 名詞 一般 * * * * 社会 シャカイ シャカイ 1 3 C1 0",
        &[(1, "良い 形容詞 自立 * * 形容詞・アウオ段 基本形 良い ヨイ ヨイ 1 2 * 0")],
    )]
    // 明日の日本です。
    #[case(
        "明日 名詞 副詞可能 * * * * 明日 アシタ アシ’タ 3 3 C2 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         日本 名詞 固有名詞 地域 国 * * 日本 ニホン ニホン 2 3 C1 0
         です 助動詞 * * * 特殊・デス 基本形 です デス デス’ 1 2 助動詞%F2@0/助詞%F2@0/連体詞%F2@0/名詞%F2@1/動詞%F1/形容詞%F2@0 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "明日 名詞 副詞可能 * * * * 明日 アス アス 2 2 C2 -1")],
    )]
    // 花弁より大きい。
    #[case(
        "花弁 名詞 一般 * * * * 花弁 ハナビラ ハナビラ 3 4 C1 -1
         より 助詞 格助詞 一般 * * * より ヨリ ヨリ 1 2 名詞%F2@1 1
         大きい 形容詞 自立 * * 形容詞・イ段 基本形 大きい オオキイ オーキイ 3 4 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "花弁 名詞 一般 * * * * 花弁 カベン カベン 4 3 C1 -1")],
    )]
    // 花弁などを摘む
    #[case(
        "花弁 名詞 一般 * * * * 花弁 ハナビラ ハナビラ 3 4 C2 -1
         など 助詞 副助詞 * * * * など ナド ナド 5 2 名詞%F2@1/動詞%F2@0 1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         摘む 動詞 自立 * * 五段・マ行 基本形 摘む ツム ツム 0 2 * 0",
        &[(0, "花弁 名詞 一般 * * * * 花弁 カベン カベン 4 3 C2 -1")],
    )]
    // 切れ味が鈍る
    #[case(
        "切れ味 名詞 一般 * * * * 切れ味 キレアジ キレアジ 2 4 C2 -1
         が 助詞 格助詞 一般 * * * が ガ ガ 0 1 名詞%F1 1
         鈍る 動詞 自立 * * 五段・ラ行 基本形 鈍る ナマル ナマル 2 3 * 0",
        &[(2, "鈍る 動詞 自立 * * 五段・ラ行 基本形 鈍る ニブル ニブル 2 3 * 0")],
    )]
    // 水の面を見つめる。
    #[case(
        "水 名詞 一般 * * * * 水 ミズ ミズ 0 2 C3 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         面 名詞 一般 * * * * 面 メン メン 0 2 C3 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         見つめる 動詞 自立 * * 一段 基本形 見つめる ミツメル ミツメル 0 4 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "面 名詞 一般 * * * * 面 オモテ オモテ 3 3 C3 0")],
    )]
    // 天地の道を学ぶ。
    #[case(
        "天地 名詞 一般 * * * * 天地 テンチ テンチ 1 3 C1 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         道 名詞 一般 * * * * 道 ミチ ミチ 0 2 C3 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         学ぶ 動詞 自立 * * 五段・バ行 基本形 学ぶ マナブ マナブ 0 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "天地 名詞 一般 * * * * 天地 アメツチ アメツチ 1 4 C1 -1")],
    )]
    // 市、町、村を数える。
    #[case(
        "市 名詞 一般 * * * * 市 シ シ 1 1 C3 -1
         、 記号 読点 * * * * 、 、 、 0 0 * 0
         町 名詞 一般 * * * * 町 マチ マチ 2 2 C3 0
         、 記号 読点 * * * * 、 、 、 0 0 * 0
         村 名詞 一般 * * * * 村 ムラ ムラ 2 2 C4 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         数える 動詞 自立 * * 一段 基本形 数える カゾエル カゾエル 3 4 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "町 名詞 一般 * * * * 町 チョウ チョー 1 2 C3 0"), (4, "村 名詞 一般 * * * * 村 ソン ソン 1 2 C4 0")],
    )]
    // 本を正す。
    #[case(
        "本 名詞 一般 * * * * 本 ホン ホン 1 2 C3 -1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         正す 動詞 自立 * * 五段・サ行 基本形 正す タダス タダス 2 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "本 名詞 一般 * * * * 本 モト モト 2 2 C3 -1")],
    )]
    // 湖の主として語られる。
    #[case(
        "湖 名詞 一般 * * * * 湖 ミズウミ ミズウミ 3 4 C1 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         主として 副詞 一般 * * * * 主として シュトシテ シュトシ’テ 1 4 * 0
         語ら 動詞 自立 * * 五段・ラ行 未然形 語る カタラ カタラ 4 3 * 0
         れる 動詞 接尾 * * 一段 基本形 れる レル レル 1 2 C1 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "主として 副詞 一般 * * * * 主として ヌシトシテ ヌシトシテ 1 5 * 0")],
    )]
    // 名前は六三四です。
    #[case(
        "名前 名詞 一般 * * * * 名前 ナマエ ナマエ 0 3 C2 -1
         は 助詞 係助詞 * * * * は ハ ワ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/助詞%F2@0 1
         六 名詞 数 * * * * 六 ロク ロク’ 3 2 C5 0
         三 名詞 数 * * * * 三 サン サン 0 2 C5 1
         四 名詞 数 * * * * 四 ヨン ヨン 1 2 C5 0
         です 助動詞 * * * 特殊・デス 基本形 です デス デス’ 1 2 助動詞%F2@0/助詞%F2@0/連体詞%F2@0/名詞%F2@1/動詞%F1/形容詞%F2@0 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "六 名詞 数 * * * * 六 ム ム 1 1 C5 0"), (3, "三 名詞 数 * * * * 三 サ サ 0 1 C5 1"), (4, "四 名詞 数 * * * * 四 シ シ 0 1 C5 1")],
    )]
    // 古い機を修理する。
    #[case(
        "古い 形容詞 自立 * * 形容詞・アウオ段 基本形 古い フルイ フルイ 3 3 * -1
         機 名詞 非自立 一般 * * * 機 キ キ 1 1 C3 1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         修理 名詞 サ変接続 * * * * 修理 シュウリ シューリ 1 3 C1 0
         する 動詞 自立 * * サ変・スル 基本形 する スル スル 0 2 * 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(1, "機 名詞 非自立 一般 * * * 機 ハタ ハタ 1 2 C3 1")],
    )]
    // 守備の要として
    #[case(
        "守備 名詞 サ変接続 * * * * 守備 シュビ シュビ 1 2 C3 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         要 名詞 一般 * * * * 要 ヨウ ヨー 1 2 C3 0
         と 助詞 格助詞 一般 * * * と ト ト 0 1 形容詞%F1/動詞%F2@0 1
         し 動詞 自立 * * サ変・スル 連用形 する シ シ’ 0 1 * 1
         て 助詞 接続助詞 * * * * て テ テ 0 1 動詞%F1/形容詞%F2@-1/名詞%F5 1",
        &[(2, "要 名詞 一般 * * * * 要 カナメ カナメ 0 3 C3 0")],
    )]
    // 歌の節を覚えた。
    #[case(
        "歌 名詞 一般 * * * * 歌 ウタ ウタ 2 2 C3 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         節 名詞 一般 * * * * 節 フシ フシ 1 2 C3 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         覚え 動詞 自立 * * 一段 連用形 覚える オボエ オボエ 2 3 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "節 名詞 一般 * * * * 節 フシ フシ 2 2 C3 0")],
    )]
    // 苦しみに堪える。
    #[case(
        "苦しみ 名詞 一般 * * * * 苦しみ クルシミ クルシミ 0 4 C2 -1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         堪える 動詞 自立 * * 一段 基本形 堪える コタエル コタエル 3 4 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "堪える 動詞 自立 * * 一段 基本形 堪える タエル タエル 2 3 * 0")],
    )]
    // この社は神を祀る。
    #[case(
        "この 連体詞 * * * * * この コノ コノ 0 2 * -1
         社 名詞 一般 * * * * 社 シャ シャ 1 1 C3 0
         は 助詞 係助詞 * * * * は ハ ワ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/助詞%F2@0 1
         神 名詞 一般 * * * * 神 カミ カミ 1 2 C3 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         祀る 動詞 自立 * * 五段・ラ行 基本形 祀る マツル マツル 0 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(1, "社 名詞 一般 * * * * 社 ヤシロ ヤシロ 1 3 C3 0")],
    )]
    // 汚れを祓う。
    #[case(
        "汚れ 名詞 一般 * * * * 汚れ ヨゴレ ヨゴレ 3 3 C2 -1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         祓う 動詞 自立 * * 五段・ワ行促音便 基本形 祓う ハラウ ハラウ 2 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "汚れ 名詞 一般 * * * * 汚れ ケガレ ケガレ 0 3 C2 -1")],
    )]
    // 厳しい訓練に音を上げた。
    #[case(
        "厳しい 形容詞 自立 * * 形容詞・イ段 基本形 厳しい キビシイ キビシイ 3 4 * -1
         訓練 名詞 サ変接続 * * * * 訓練 クンレン クンレン 1 4 C1 0
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         音 名詞 一般 * * * * 音 オト オト 2 2 C3 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         上げ 動詞 自立 * * 一段 連用形 上げる アゲ アゲ 3 2 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(3, "音 名詞 一般 * * * * 音 ネ ネ 0 1 C3 0")],
    )]
    // 紅色の着物を着る。
    #[case(
        "紅色 名詞 一般 * * * * 紅色 コウショク コーショク 0 4 C2 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         着物 名詞 一般 * * * * 着物 キモノ キモノ 0 3 C2 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         着る 動詞 自立 * * 一段 基本形 着る キル キル 0 2 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "紅色 名詞 一般 * * * * 紅色 ベニイロ ベニイロ 0 4 C2 -1")],
    )]
    // 盲・聾・養護学校
    #[case(
        "盲 名詞 一般 * * * * 盲 メクラ メクラ 3 3 C2 -1
         ・ 記号 読点 * * * * ・ 、 、 0 0 * 0
         聾 名詞 一般 * * * * 聾 ツンボ ツンボ 1 3 C3 0
         ・ 記号 読点 * * * * ・ 、 、 0 0 * 0
         養護 名詞 サ変接続 * * * * 養護 ヨウゴ ヨーゴ 4 3 C1 0
         学校 名詞 一般 * * * * 学校 ガッコウ ガッコー 0 4 C2 1",
        &[(0, "盲 名詞 一般 * * * * 盲 モウ モー 1 2 C2 -1"), (2, "聾 名詞 一般 * * * * 聾 ロウ ロー 1 2 C3 0")],
    )]
    // 一番好きだ
    #[case(
        "一番 名詞 一般 * * * * 一番 イチバン イチバン 0 4 C1 -1
         好き 名詞 接尾 形容動詞語幹 * * * 好き ズキ ズキ 2 2 C4 1
         だ 助動詞 * * * 特殊・ダ 基本形 だ ダ ダ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/連体詞%F2@0 1",
        &[(1, "好き 名詞 接尾 形容動詞語幹 * * * 好き スキ スキ 2 2 C4 1")],
    )]
    // 面を上げよ
    #[case(
        "面 名詞 一般 * * * * 面 メン メン 0 2 C3 -1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         上げよ 動詞 自立 * * 一段 命令ｙｏ 上げる アゲヨ アゲヨ 0 3 * 0",
        &[(0, "面 名詞 一般 * * * * 面 オモテ オモテ 3 3 C3 -1")],
    )]
    // 船が覆った
    #[case(
        "船 名詞 一般 * * * * 船 フネ フネ 1 2 C3 -1
         が 助詞 格助詞 一般 * * * が ガ ガ 0 1 名詞%F1 1
         覆っ 動詞 自立 * * 五段・ワ行促音便 連用タ接続 覆う オオッ オオッ 4 3 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1",
        &[(2, "覆っ 動詞 自立 * * 五段・ラ行 連用タ接続 覆る クツガエッ クツガエッ 3 5 * 0")],
    )]
    // 崖の際を
    #[case(
        "崖 名詞 一般 * * * * 崖 ガケ ガケ 4 2 C4 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         際 名詞 非自立 副詞可能 * * * 際 サイ サイ 1 2 F2@1 1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1",
        &[(0, "崖 名詞 一般 * * * * 崖 ガケ ガケ 0 2 C4 -1"), (2, "際 名詞 一般 * * * * 際 キワ キワ 2 2 C3 0")],
    )]
    // 声明を唱える
    #[case(
        "声明 名詞 サ変接続 * * * * 声明 セイメイ セーメー 0 4 C2 -1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         唱える 動詞 自立 * * 一段 基本形 唱える トナエル トナエル 3 4 * 0",
        &[(0, "声明 名詞 サ変接続 * * * * 声明 ショウミョウ ショーミョー 1 4 C2 -1")],
    )]
    // 生成りの麻布
    #[case(
        "生成り 名詞 一般 * * * * 生成り キナリ キナリ 0 3 C2 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         麻布 名詞 固有名詞 地域 一般 * * 麻布 アザブ アザブ 0 3 * 0",
        &[(2, "麻布 名詞 固有名詞 地域 一般 * * 麻布 アサヌノ アサヌノ 0 4 * 0")],
    )]
    // 二十前
    #[case(
        "二 名詞 数 * * * * 二 ニ ニ 4 1 C3 0
         十 名詞 数 * * * * 十 ジュウ ジュー 1 2 * 1
         前 名詞 接尾 一般 * * * 前 マエ マエ 1 2 C1 1",
        &[(0, "二 名詞 数 * * * * 二 ハタ ハタ 4 2 C3 0"), (1, "十 名詞 数 * * * * 十 チ チ 1 1 * 1")],
    )]
    // 二十を過ぎる。
    #[case(
        "二 名詞 数 * * * * 二 ニ ニ 1 1 C3 0
         十 名詞 数 * * * * 十 ジュウ ジュー 1 2 * 1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         過ぎる 動詞 自立 * * 一段 基本形 過ぎる スギル スギル 2 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "二 名詞 数 * * * * 二 ハタ ハタ 1 2 C3 0"), (1, "十 名詞 数 * * * * 十 チ チ 1 1 * 1")],
    )]
    // 九つ、十
    #[case(
        "九つ 名詞 一般 * * * * 九つ ココノツ ココノツ 2 4 C1 -1
         、 記号 読点 * * * * 、 、 、 0 0 * 0
         十 名詞 数 * * * * 十 ジュウ ジュー 1 2 C3 0",
        &[(2, "十 名詞 数 * * * * 十 トオ トー 1 2 C3 0")],
    )]
    // 1日中働いた。
    #[case(
        "一日 名詞 副詞可能 * * * * 一日 イチニチ イチニチ’ 0 4 * 0
         中 名詞 接尾 副詞可能 * * * 中 チュウ チュー 1 2 C4 1
         働い 動詞 自立 * * 五段・カ行イ音便 連用タ接続 働く ハタライ ハタライ 5 4 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(1, "中 名詞 接尾 副詞可能 * * * 中 ジュウ ジュー 1 2 C4 1")],
    )]
    // 月〜金
    #[case(
        "月 名詞 一般 * * * * 月 ツキ ツ’キ 2 2 C3 -1
         〜 記号 読点 * * * * 〜 、 、 0 0 * 0
         金 名詞 一般 * * * * 金 カネ カネ 0 2 C3 0",
        &[(2, "金 名詞 一般 * * * * 金 キン キン 0 2 C3 0")],
    )]
    // 領収書に金五万円と書いた
    #[case(
        "領収 名詞 サ変接続 * * * * 領収 リョウシュウ リョーシュー 0 4 C2 -1
         書 名詞 接尾 一般 * * * 書 ショ ショ 1 1 C4 1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         金 名詞 一般 * * * * 金 カネ カネ 0 2 C3 0
         五 名詞 数 * * * * 五 ゴ ゴ 0 1 C3 0
         万 名詞 数 * * * * 万 マン マン 1 2 C1 1
         円 名詞 接尾 助数詞 * * * 円 エン エン 1 2 F5 1
         と 助詞 格助詞 引用 * * * と ト ト 0 1 形容詞%F1/動詞%F2@0 1
         書い 動詞 自立 * * 五段・カ行イ音便 連用タ接続 書く カイ カイ 1 2 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1",
        &[(3, "金 名詞 一般 * * * * 金 キン キン 0 2 C3 0")],
    )]
    // 保護房に入る。
    #[case(
        "保護 名詞 サ変接続 * * * * 保護 ホゴ ホゴ 2 2 C3 -1
         房 名詞 一般 * * * * 房 フサ フ’サ 2 2 C3 1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         入る 動詞 自立 * * 五段・ラ行 基本形 入る ハイル ハイル 1 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(1, "房 名詞 一般 * * * * 房 ボウ ボー 2 2 C3 1")],
    )]
    // 五月期の決算。
    #[case(
        "五月 名詞 一般 * * * * 五月 サツキ サツ’キ 3 3 C2 -1
         期 名詞 接尾 一般 * * * 期 キ キ 1 1 C3 1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         決算 名詞 サ変接続 * * * * 決算 ケッサン ケッサン 1 4 C1 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "五月 名詞 一般 * * * * 五月 ゴガツ ゴガツ 3 3 C2 -1")],
    )]
    // 都大会に出る。
    #[case(
        "都 名詞 一般 * * * * 都 ミヤコ ミヤコ 4 3 C3 -1
         大会 名詞 一般 * * * * 大会 タイカイ タイカイ 0 4 C2 1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         出る 動詞 自立 * * 一段 基本形 出る デル デル 1 2 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "都 名詞 一般 * * * * 都 ト ト 2 1 C3 -1")],
    )]
    // 露対テロ作戦。
    #[case(
        "露 副詞 一般 * * * * 露 ツユ ツユ 1 2 * -1
         対 接頭詞 名詞接続 * * * * 対 タイ タイ 5 2 P1 0
         テロ 名詞 一般 * * * * テロ テロ テロ 1 2 C3 1
         作戦 名詞 一般 * * * * 作戦 サクセン サク’セン 0 4 C2 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "露 副詞 一般 * * * * 露 ロ ロ 1 1 * -1")],
    )]
    // あなた方の意見。
    #[case(
        "あなた 名詞 代名詞 一般 * * * あなた アナタ アナタ 3 3 C1 -1
         方 名詞 接尾 一般 * * * 方 カタ カタ 1 2 C3 1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         意見 名詞 サ変接続 * * * * 意見 イケン イケン 1 3 C1 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(1, "方 名詞 接尾 一般 * * * 方 ガタ ガタ 1 2 C3 1")],
    )]
    // ルモンド（仏）が報じた。
    #[case(
        "ルモンド 名詞 固有名詞 一般 * * * ルモンド ルモンド ルモンド 2 4 C1 -1
         （ 記号 読点 * * * * （ 、 、 0 0 * 0
         仏 名詞 一般 * * * * 仏 ホトケ ホトケ 0 3 C2 0
         ） 記号 読点 * * * * ） 、 、 0 0 * 0
         が 助詞 格助詞 一般 * * * が ガ ガ 0 1 名詞%F1 0
         報じ 動詞 自立 * * 一段 連用形 報じる ホウジ ホージ 4 3 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(2, "仏 名詞 一般 * * * * 仏 フツ フツ 1 2 C2 0")],
    )]
    // 山田太郎（仮名）は語った。
    #[case(
        "山田 名詞 固有名詞 人名 姓 * * 山田 ヤマダ ヤマダ 0 3 C2 -1
         太郎 名詞 固有名詞 人名 名 * * 太郎 タロウ タロー 1 3 C1 0
         （ 記号 読点 * * * * （ 、 、 0 0 * 0
         仮名 名詞 一般 * * * * 仮名 カナ カナ 0 2 C3 0
         ） 記号 読点 * * * * ） 、 、 0 0 * 0
         は 助詞 係助詞 * * * * は ハ ワ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/助詞%F2@0 0
         語っ 動詞 自立 * * 五段・ラ行 連用タ接続 語る カタッ カタッ 4 3 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(3, "仮名 名詞 一般 * * * * 仮名 カメイ カメー 0 3 C3 0")],
    )]
    // 都は費用を助成する。
    #[case(
        "都 名詞 一般 * * * * 都 ミヤコ ミヤコ 0 3 C3 -1
         は 助詞 係助詞 * * * * は ハ ワ 0 1 名詞%F1/動詞%F2@0/形容詞%F2@0/助詞%F2@0 1
         費用 名詞 一般 * * * * 費用 ヒヨウ ヒヨー 1 3 C1 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         助成 名詞 サ変接続 * * * * 助成 ジョセイ ジョセー 0 3 C2 0
         する 動詞 自立 * * サ変・スル 基本形 する スル スル 0 2 * 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "都 名詞 一般 * * * * 都 ト ト 1 1 C3 -1")],
    )]
    // 抽選に当たります様に。
    #[case(
        "抽選 名詞 サ変接続 * * * * 抽選 チュウセン チューセン 0 4 C1 -1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         当たり 動詞 自立 * * 五段・ラ行 連用形 当たる アタリ アタリ 4 3 * 0
         ます 助動詞 * * * 特殊・マス 基本形 ます マス マス’ 1 2 動詞%F2@1/助詞%F2@1 1
         様 名詞 接尾 人名 * * * 様 サマ サマ 2 2 C5 1
         に 助詞 格助詞 一般 * * * に ニ ニ 0 1 動詞%F2@1/形容詞%F1/名詞%F1 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(4, "様 名詞 接尾 人名 * * * 様 ヨウ ヨー 2 2 C5 1")],
    )]
    // 神の代
    #[case(
        "神 名詞 一般 * * * * 神 カミ カミ 1 2 C3 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         代 名詞 一般 * * * * 代 ダイ ダイ 1 2 C3 0",
        &[(2, "代 名詞 一般 * * * * 代 ヨ ヨ 1 1 C3 0")],
    )]
    // 番号の下四桁を入力する
    #[case(
        "番号 名詞 一般 * * * * 番号 バンゴウ バンゴー 3 4 C1 -1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         下 名詞 一般 * * * * 下 シタ シ’タ 0 2 C3 0
         四 名詞 数 * * * * 四 ヨン ヨン 1 2 C1 0
         桁 名詞 接尾 助数詞 * * * 桁 ケタ ケタ 0 2 F4@-1 1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         入力 名詞 サ変接続 * * * * 入力 ニュウリョク ニューリョク’ 0 4 C2 0
         する 動詞 自立 * * サ変・スル 基本形 する スル スル 0 2 * 1",
        &[(2, "下 名詞 一般 * * * * 下 シモ シモ 0 2 C3 0")],
    )]
    // 視入った
    #[case(
        "視 動詞 自立 * * 一段 連用形 視る ミ ミ 2 1 * -1
         入っ 動詞 自立 * * 五段・ラ行 連用タ接続 入る ハイッ ハイッ 1 3 C1 1
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1",
        &[(1, "入っ 動詞 自立 * * 五段・ラ行 連用タ接続 入る イッ イッ 1 2 C1 1")],
    )]
    // 放っておく。
    #[case(
        "放っ 動詞 自立 * * 五段・タ行 連用タ接続 放つ ハナッ ハナッ 0 3 * -1
         て 助詞 接続助詞 * * * * て テ テ 0 1 動詞%F1/形容詞%F2@-1/名詞%F5 1
         おく 動詞 非自立 * * 五段・カ行イ音便 基本形 おく オク オク 0 2 動詞%F2@1 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(0, "放っ 動詞 自立 * * 五段・タ行 連用タ接続 放つ ホウッ ホーッ 0 3 * -1")],
    )]
    // 服従を強いた
    #[case(
        "服従 名詞 サ変接続 * * * * 服従 フクジュウ フ’クジュー 0 4 C2 -1
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         強い 形容詞 自立 * * 形容詞・アウオ段 基本形 強い ツヨイ ツヨイ 2 3 * 0
         た 助動詞 * * * 特殊・タ 基本形 た タ タ 0 1 助詞%F2@0/助動詞%F2@0/動詞%F2@1/形容詞%F4@-2 1",
        &[(2, "強い 形容詞 自立 * * 形容詞・アウオ段 基本形 強い シイ シイ 2 2 * 0")],
    )]
    // 謹んで申し上げ候
    #[case(
        "謹んで 副詞 一般 * * * * 謹んで ツツシンデ ツ’ツシンデ 3 5 * -1
         申し上げ 動詞 自立 * * サ変・スル 連用形 する モウシアゲ モーシアゲ 4 5 * 1
         候 名詞 一般 * * * * 候 コウ コー 1 2 C3 0",
        &[(2, "候 名詞 一般 * * * * 候 ソウロウ ソーロー 1 4 C3 0")],
    )]
    // 六月十二日付の通知を送る。
    #[case(
        "六月 名詞 副詞可能 * * * * 六月 ロクガツ ロクガツ 4 4 C2 -1
         十 名詞 数 * * * * 十 ジュウ ジュー 0 2 C3 0
         二 名詞 数 * * * * 二 ニ ニ 1 1 C1 1
         日 名詞 接尾 助数詞 * * * 日 ニチ ニチ 1 2 F4@2 1
         付 名詞 接尾 一般 * * * 付 ヅケ ズケ 0 2 C4 1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         通知 名詞 サ変接続 * * * * 通知 ツウチ ツーチ 0 3 C2 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         送る 動詞 自立 * * 五段・ラ行 基本形 送る オクル オクル 0 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[],
    )]
    // 二十日付で異動する。
    #[case(
        "二十日 名詞 副詞可能 * * * * 二十日 ハツカ ハツ’カ 0 3 * 0
         付 名詞 接尾 一般 * * * 付 ヅケ ズケ 0 2 C4 1
         で 助詞 格助詞 一般 * * * で デ デ 1 1 動詞%F1 1
         異動 名詞 サ変接続 * * * * 異動 イドウ イドー 0 3 C2 0
         する 動詞 自立 * * サ変・スル 基本形 する スル スル 0 2 * 1
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[],
    )]
    // 条件付の契約を結ぶ。
    #[case(
        "条件 名詞 一般 * * * * 条件 ジョウケン ジョーケン 0 4 C1 -1
         付 名詞 接尾 一般 * * * 付 ヅケ ズケ 0 2 C4 1
         の 助詞 連体化 * * * * の ノ ノ 1 1 助動詞%F2@0/助詞%F2@0/動詞%F2@1/形容詞%F1 1
         契約 名詞 サ変接続 * * * * 契約 ケイヤク ケーヤク 0 4 C2 0
         を 助詞 格助詞 一般 * * * を ヲ ヲ 0 1 動詞%F5/名詞%F1 1
         結ぶ 動詞 自立 * * 五段・バ行 基本形 結ぶ ムスブ ムスブ 0 3 * 0
         。 記号 読点 * * * * 。 、 、 0 0 * 0",
        &[(1, "付 名詞 接尾 一般 * * * 付 ツキ ツキ 0 2 C4 1")],
    )]
    fn modify_context_reading_matches_pyopenjtalk_plus(
        #[case] features: &str,
        #[case] changes: &[(usize, &str)],
    ) {
        let features = parse_full_nodes(features);
        let mut expected = features.clone();
        for &(index, line) in changes {
            expected[index] = parse_full_node(line);
        }
        assert_eq!(expected, super::modify_context_reading(features));
    }
}
